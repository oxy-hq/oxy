use crate::adapters::openai::{AsyncFunctionObject, IntoOpenAIConfig};
use crate::agent::OpenAIExecutableResponse;
use crate::agent::builders::openai::{OpenAIExecutable, build_openai_executable};
use crate::agent::builders::tool::OpenAITool;
use crate::agent::contexts::Contexts;
use crate::agent::databases::DatabasesContext;
use crate::config::ConfigManager;
use crate::config::constants::AGENT_SOURCE_PROMPT;
use crate::config::model::{AgentContext, AgentToolsConfig, DefaultAgent, ReasoningConfig};
use crate::execute::builders::map::ParamMapper;
use crate::execute::renderer::Renderer;
use crate::execute::types::{Output, OutputContainer};
use crate::semantic::SemanticManager;
use crate::service::agent::Message;
use crate::tools::ToolsContext;
use crate::{
    adapters::openai::OpenAIClient,
    config::model::ToolType,
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::ExecutableBuilder,
        types::{Chunk, Prompt},
    },
};
use async_openai::types::{
    ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
    ChatCompletionTool,
};
use minijinja::{Value, context};

#[derive(Debug, Clone)]
pub(super) struct DefaultAgentExecutable;

#[derive(Debug, Clone)]
pub struct DefaultAgentInput {
    pub agent_name: String,
    pub model: String,
    pub default_agent: DefaultAgent,
    pub contexts: Option<Vec<AgentContext>>,
    pub prompt: String,
    pub memory: Vec<Message>,
    pub reasoning_config: Option<ReasoningConfig>,
}

#[async_trait::async_trait]
impl Executable<DefaultAgentInput> for DefaultAgentExecutable {
    type Response = OutputContainer;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: DefaultAgentInput,
    ) -> Result<Self::Response, OxyError> {
        let DefaultAgentInput {
            agent_name,
            model,
            prompt,
            contexts: _,
            default_agent:
                DefaultAgent {
                    system_instructions,
                    tools_config:
                        AgentToolsConfig {
                            tools,
                            max_tool_calls,
                            max_tool_concurrency,
                        },
                },
            memory,
            reasoning_config,
        } = input;
        tracing::debug!("Default agent input: {:?}", &memory);
        let model_config = execution_context
            .project
            .config_manager
            .resolve_model(&model)
            .map_err(|e| {
                OxyError::ConfigurationError(format!("Failed to resolve model config: {e}"))
            })?;
        let system_instructions = execution_context
            .renderer
            .render_async(&system_instructions)
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to render system instructions: {e}"))
            })?;
        tracing::info!(
            "Executing default agent: {} with model: {}",
            agent_name,
            model_config.model_name()
        );
        tracing::info!("System instructions: {}", system_instructions);

        // Render all tool configurations with variables
        let mut rendered_tools = Vec::new();
        for tool in tools.iter() {
            let rendered_tool = tool.render(&execution_context.renderer).await?;
            rendered_tools.push(rendered_tool);
        }

        let client = OpenAIClient::with_config(
            model_config
                .into_openai_config(&execution_context.project.secrets_manager)
                .await?,
        );
        let messages: Result<Vec<ChatCompletionRequestMessage>, OxyError> = memory
            .into_iter()
            .map(
                |message| -> Result<ChatCompletionRequestMessage, OxyError> {
                    let result = if message.is_human {
                        ChatCompletionRequestUserMessageArgs::default()
                            .content(message.content)
                            .build()
                            .map_err(|e| {
                                OxyError::RuntimeError(format!(
                                    "Failed to build user message from memory: {e}"
                                ))
                            })?
                            .into()
                    } else {
                        ChatCompletionRequestAssistantMessageArgs::default()
                            .content(message.content)
                            .build()
                            .map_err(|e| {
                                OxyError::RuntimeError(format!(
                                    "Failed to build assistant message from memory: {e}"
                                ))
                            })?
                            .into()
                    };
                    Ok(result)
                },
            )
            .collect();
        let mut messages = messages?;
        messages.extend(vec![
            ChatCompletionRequestSystemMessageArgs::default()
                .content(system_instructions)
                .build()
                .map_err(|e| {
                    OxyError::RuntimeError(format!("Failed to build system message: {e}"))
                })?
                .into(),
            ChatCompletionRequestUserMessageArgs::default()
                .content(prompt.clone())
                .build()
                .map_err(|e| OxyError::RuntimeError(format!("Failed to build user message: {e}")))?
                .into(),
        ]);
        execution_context
            .write_chunk(Chunk {
                key: Some(AGENT_SOURCE_PROMPT.to_string()),
                delta: Prompt::new(prompt.clone()).into(),
                finished: true,
            })
            .await?;
        let config = execution_context.project.config_manager.clone();
        let mut react_executable = build_react_loop(
            agent_name,
            rendered_tools,
            max_tool_concurrency,
            client,
            model_config.model_name().to_string(),
            max_tool_calls,
            &config,
            reasoning_config,
        )
        .await;
        let outputs = react_executable
            .execute(execution_context, messages)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute react loop: {e}")))?;
        let output = outputs
            .into_iter()
            .fold(Output::default(), |m, o| m.merge(&o.content));
        Ok(output.into())
    }
}

async fn build_react_loop(
    agent_name: String,
    tool_configs: Vec<ToolType>,
    max_concurrency: usize,
    client: OpenAIClient,
    model: String,
    max_iterations: usize,
    config: &ConfigManager,
    reasoning_config: Option<ReasoningConfig>,
) -> impl Executable<Vec<ChatCompletionRequestMessage>, Response = Vec<OpenAIExecutableResponse>> {
    let tools: Vec<ChatCompletionTool> = futures::future::join_all(
        tool_configs
            .iter()
            .map(|tool| ChatCompletionTool::from_tool_async(tool, config)),
    )
    .await
    .into_iter()
    .collect();
    ExecutableBuilder::new()
        .react(
            OpenAITool::new(agent_name, tool_configs, max_concurrency),
            max_iterations,
        )
        .memo(vec![])
        .executable(build_openai_executable(
            client,
            model,
            tools,
            None,
            reasoning_config.map(|c| c.into()),
            false,
        ))
}

#[derive(Clone)]
pub struct DefaultAgentMapper;

#[async_trait::async_trait]
impl ParamMapper<DefaultAgentInput, DefaultAgentInput> for DefaultAgentMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: DefaultAgentInput,
    ) -> Result<(DefaultAgentInput, Option<ExecutionContext>), OxyError> {
        let default_agent = &input.default_agent;
        let global_context = build_global_context(
            execution_context,
            &input.agent_name,
            default_agent,
            input.contexts.clone().unwrap_or_default(),
            &input.prompt,
        )
        .await?;

        // Merge the existing current_context (which contains variables) with the new global_context
        let existing_context = execution_context.renderer.get_context();
        let merged_context = context! {
            ..global_context,
            ..existing_context,
        };

        let renderer = Renderer::from_template(
            merged_context,
            &input.default_agent.system_instructions.as_str(),
        )
        .map_err(|e| {
            OxyError::RuntimeError(format!("Failed to create renderer from template: {e}"))
        })?;
        let execution_context = execution_context.wrap_renderer(renderer);
        Ok((input, Some(execution_context)))
    }
}

pub async fn build_global_context(
    execution_context: &ExecutionContext,
    agent_name: &str,
    default_agent: &DefaultAgent,
    contexts: Vec<AgentContext>,
    prompt: &str,
) -> Result<Value, OxyError> {
    let config = execution_context.project.config_manager.clone();
    let secrets_manager = execution_context.project.secrets_manager.clone();
    let contexts = Contexts::new(contexts, config.clone());
    let databases = DatabasesContext::new(config.clone(), secrets_manager.clone());
    let tools = ToolsContext::from_execution_context(
        execution_context,
        agent_name.to_string(),
        default_agent.tools_config.tools.clone(),
        prompt.to_string(),
    );
    let semantic_manager = SemanticManager::from_config(config, secrets_manager, false).await?;
    let semantic_contexts = semantic_manager.get_semantic_variables_contexts().await?;
    let semantic_dimensions_contexts = semantic_manager
        .get_semantic_dimensions_contexts(&semantic_contexts)
        .await?;

    // Get globals from the semantic manager
    let globals_value = semantic_manager.get_globals_value()?;

    // Convert serde_yaml::Value to minijinja::Value
    let globals = Value::from_serialize(&globals_value);

    Ok(context! {
        context => Value::from_object(contexts),
        databases => Value::from_object(databases),
        models => Value::from_object(semantic_contexts),
        dimensions => Value::from_object(semantic_dimensions_contexts),
        tools => Value::from_object(tools),
        globals => globals,
    })
}

pub(super) fn build_default_agent_executable()
-> impl Executable<DefaultAgentInput, Response = OutputContainer> {
    ExecutableBuilder::new()
        .map(DefaultAgentMapper)
        .executable(DefaultAgentExecutable)
}
