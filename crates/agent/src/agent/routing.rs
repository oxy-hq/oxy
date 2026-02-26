use async_openai::types::chat::ChatCompletionTool;

use super::openai::{OneShotInput, SimpleMapper, build_openai_executable};
use super::routing_fallback::FallbackAgent;
use super::tool::OpenAITool;
use crate::{agent::openai::OpenAIExecutableResponse, routing::RouteResolver, types::Message};

use oxy::{
    adapters::{
        openai::{AsyncFunctionObject, IntoOpenAIConfig, OpenAIClient},
        secrets::SecretsManager,
    },
    config::{
        ConfigManager,
        constants::ARTIFACT_SOURCE,
        model::{Model, ReasoningConfig, RoutingAgent, ToolType},
    },
    execute::{
        Executable, ExecutionContext,
        builders::ExecutableBuilder,
        types::{Event, OutputContainer},
    },
    observability::events,
};
use oxy_shared::errors::OxyError;

#[derive(Debug, Clone, serde::Serialize)]
pub struct RoutingAgentInput {
    pub agent_name: String,
    pub model: String,
    pub routing_agent: RoutingAgent,
    pub prompt: String,
    pub memory: Vec<Message>,
    pub reasoning_config: Option<ReasoningConfig>,
}

#[allow(dead_code)]
pub struct OmniRoute {
    pub integration_name: String,
    pub topic_pattern: Option<String>, // None means full wildcard
}

#[derive(Debug, Clone)]
pub(super) struct RoutingAgentExecutable;

#[async_trait::async_trait]
impl Executable<RoutingAgentInput> for RoutingAgentExecutable {
    type Response = OutputContainer;

    #[tracing::instrument(skip_all, err, fields(
        otel.name = events::agent::routing_agent::NAME,
        oxy.span_type = events::agent::routing_agent::TYPE,
        oxy.agent.name = %input.agent_name,
    ))]
    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: RoutingAgentInput,
    ) -> Result<Self::Response, OxyError> {
        events::agent::routing_agent::input(&input);

        let RoutingAgentInput {
            agent_name,
            model,
            routing_agent,
            prompt,
            memory,
            reasoning_config,
        } = input;

        let config_manager = &execution_context.project.config_manager;
        let secrets_manager = &execution_context.project.secrets_manager;
        let model_config = config_manager.resolve_model(&model)?;

        events::agent::routing_agent::model_config(model_config);

        events::agent::routing_agent::system_instructions(&routing_agent.system_instructions);

        let tool_configs = RouteResolver::resolve_routes(
            execution_context,
            &agent_name,
            &routing_agent.db_config,
            &routing_agent.embedding_config,
            // RoutingAgent has no api_url/key_var fields; falls back to OpenAI defaults.
            // TODO: add api_url/key_var to RoutingAgent for non-OpenAI embedding providers.
            "https://api.openai.com/v1",
            "OPENAI_API_KEY",
            &prompt,
        )
        .await?;

        events::agent::routing_agent::resolved_routes(tool_configs.len(), &tool_configs);

        // Render all tool configurations with variables
        let mut rendered_tools = Vec::new();
        for tool in tool_configs.iter() {
            let rendered_tool = tool.render(&execution_context.renderer).await?;
            rendered_tools.push(rendered_tool);
        }

        events::agent::routing_agent::tools(&rendered_tools);

        let mut react_loop_executable = build_react_loop(
            agent_name.clone(),
            model_config,
            rendered_tools,
            routing_agent.synthesize_results,
            config_manager,
            secrets_manager,
            reasoning_config.clone(),
        )
        .await?;
        let one_shot_input = OneShotInput {
            system_instructions: routing_agent.system_instructions.clone(),
            user_input: Some(prompt),
            memory,
        };
        let outputs = match routing_agent.route_fallback {
            Some(ref fallback) => {
                events::agent::routing_agent::fallback_configured(fallback);

                let fallback_tool =
                    RouteResolver::resolve_tool(execution_context, fallback, None, false).await?;

                let config_manager = &execution_context.project.config_manager;
                let secrets_manager = &execution_context.project.secrets_manager;
                let fallback_route = FallbackAgent::new(
                    &agent_name,
                    model_config,
                    fallback_tool,
                    config_manager,
                    secrets_manager,
                    reasoning_config,
                )
                .await?;
                let mut fallback_executable = build_fallback(react_loop_executable, fallback_route);
                fallback_executable
                    .execute(execution_context, one_shot_input)
                    .await
            }
            None => {
                react_loop_executable
                    .execute(execution_context, one_shot_input)
                    .await
            }
        }?;

        let output_container =
            OutputContainer::List(outputs.into_iter().map(|o| o.content.into()).collect());

        events::agent::routing_agent::output(&output_container);

        Ok(output_container)
    }
}

async fn build_react_loop(
    agent_name: String,
    model: &Model,
    tool_configs: Vec<ToolType>,
    synthesize_results: bool,
    config: &ConfigManager,
    secrets_manager: &SecretsManager,
    reasoning_config: Option<ReasoningConfig>,
) -> Result<impl Executable<OneShotInput, Response = Vec<OpenAIExecutableResponse>> + Clone, OxyError>
{
    let tools: Vec<ChatCompletionTool> = futures::future::join_all(
        tool_configs
            .iter()
            .map(|tool| ChatCompletionTool::from_tool_async(tool, config)),
    )
    .await
    .into_iter()
    .collect();
    let builder = match synthesize_results {
        true => ExecutableBuilder::new()
            .map(SimpleMapper)
            .react_rar(OpenAITool::new(agent_name, tool_configs, 1)),
        false => ExecutableBuilder::new()
            .map(SimpleMapper)
            .react_once(OpenAITool::new(agent_name, tool_configs, 1)),
    };

    let client = OpenAIClient::with_config(model.into_openai_config(secrets_manager).await?);
    let deduplicated_tools = deduplicate_tools(tools)?;
    Ok(builder.memo(vec![]).executable(build_openai_executable(
        client,
        model.model_name().to_string(),
        deduplicated_tools,
        None,
        reasoning_config,
        synthesize_results,
    )))
}

fn build_fallback(
    executable: impl Executable<OneShotInput, Response = Vec<OpenAIExecutableResponse>> + Send,
    fallback: FallbackAgent,
) -> impl Executable<OneShotInput, Response = Vec<OpenAIExecutableResponse>> {
    ExecutableBuilder::default()
        .fallback(
            |response: &Vec<OpenAIExecutableResponse>| {
                response.iter().any(|r| !r.tool_calls.is_empty())
            },
            |event: &Event| event.source.kind.as_str() == ARTIFACT_SOURCE,
            ExecutableBuilder::new()
                .map(SimpleMapper)
                .executable(fallback),
        )
        .executable(executable)
}

/// Deduplicates tools by their function names, ensuring each tool has a unique name.
///
/// When duplicate tool names are found, this function appends a numeric suffix
/// (e.g., "tool_1", "tool_2") to make them unique. The original tool keeps its
/// name if it's the first occurrence.
///
/// # Arguments
/// * `tools` - Vector of ChatCompletionTool instances that may contain duplicates
///
/// # Returns
/// * `Ok(Vec<ChatCompletionTool>)` - Deduplicated tools with unique names
/// * `Err(OxyError)` - Currently never returns an error, but maintains Result for consistency
///
/// # Example
/// If input contains tools named ["search", "search", "format"], the output will
/// contain tools named ["search", "search_1", "format"].
fn deduplicate_tools(tools: Vec<ChatCompletionTool>) -> Result<Vec<ChatCompletionTool>, OxyError> {
    let mut seen_names = std::collections::HashSet::new();
    let mut deduplicated_tools = Vec::with_capacity(tools.len());

    for tool in tools {
        let original_name = &tool.function.name;

        if seen_names.insert(original_name.clone()) {
            // First occurrence - keep original name
            deduplicated_tools.push(tool);
        } else {
            // Duplicate found - generate unique name
            let unique_name = generate_unique_tool_name(original_name, &seen_names);
            seen_names.insert(unique_name.clone());

            let mut unique_tool = tool;
            unique_tool.function.name = unique_name;
            deduplicated_tools.push(unique_tool);
        }
    }

    Ok(deduplicated_tools)
}

/// Generates a unique tool name by appending a numeric suffix.
///
/// # Arguments
/// * `base_name` - The original tool name to make unique
/// * `seen_names` - Set of already used names to avoid conflicts
///
/// # Returns
/// A unique name in the format "{base_name}_{number}"
fn generate_unique_tool_name(
    base_name: &str,
    seen_names: &std::collections::HashSet<String>,
) -> String {
    let mut suffix = 1;
    loop {
        let candidate_name = format!("{base_name}_{suffix}");
        if !seen_names.contains(&candidate_name) {
            return candidate_name;
        }
        suffix += 1;
    }
}
