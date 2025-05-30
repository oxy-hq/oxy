use std::path::PathBuf;

use async_openai::types::ChatCompletionTool;
use fallback::FallbackAgent;

use crate::{
    adapters::{
        openai::{AsyncFunctionObject, OpenAIClient},
        vector_store::parse_sql_source_type,
    },
    agent::{
        OpenAIExecutableResponse,
        builders::{
            openai::{OneShotInput, OpenAIExecutable, SimpleMapper},
            tool::OpenAITool,
        },
    },
    config::{
        constants::ARTIFACT_SOURCE,
        model::{AgentTool, ExecuteSQLTool, Model, RoutingAgent, ToolType, WorkflowTool},
    },
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::ExecutableBuilder,
        types::{Document, Event, OutputContainer},
    },
    service::agent::Message,
    tools::{RetrievalExecutable, types::RetrievalInput},
    utils::to_openai_function_name,
};

mod fallback;

#[derive(Debug, Clone)]
pub(super) struct RoutingAgentInput {
    pub agent_name: String,
    pub model: String,
    pub routing_agent: RoutingAgent,
    pub prompt: String,
    pub memory: Vec<Message>,
}

#[derive(Debug, Clone)]
pub(super) struct RoutingAgentExecutable;

impl RoutingAgentExecutable {
    async fn resolve_tool(
        &self,
        execution_context: &ExecutionContext,
        file_ref: &str,
        is_verified: bool,
    ) -> Result<ToolType, OxyError> {
        match file_ref {
            workflow_path if workflow_path.ends_with(".workflow.yml") => {
                let workflow = execution_context
                    .config
                    .resolve_workflow(workflow_path)
                    .await?;
                Ok(ToolType::Workflow(WorkflowTool {
                    name: to_openai_function_name(&PathBuf::from(workflow_path))?,
                    workflow_ref: workflow_path.to_string(),
                    variables: workflow.variables,
                    description: workflow.description,
                    output_task_ref: None,
                    is_verified,
                }))
            }
            agent_path if agent_path.ends_with(".agent.yml") => {
                let agent = execution_context.config.resolve_agent(agent_path).await?;
                Ok(ToolType::Agent(AgentTool {
                    name: to_openai_function_name(&PathBuf::from(agent_path))?,
                    agent_ref: agent_path.to_string(),
                    description: agent.description,
                    is_verified,
                }))
            }
            _ => Err(OxyError::AgentError(format!(
                "Unsupported tool type for path: {}",
                file_ref
            ))),
        }
    }

    async fn resolve_document(
        &self,
        execution_context: &ExecutionContext,
        document: &Document,
    ) -> Result<ToolType, OxyError> {
        match document.id.as_str() {
            sql_path if sql_path.ends_with(".sql") => {
                if let Some(database_ref) = parse_sql_source_type(document.kind.as_str()) {
                    execution_context.config.resolve_database(&database_ref)?;
                    Ok(ToolType::ExecuteSQL(ExecuteSQLTool {
                        database: database_ref.to_string(),
                        description: document.content.to_string(),
                        dry_run_limit: None,
                        name: to_openai_function_name(&PathBuf::from(sql_path))?,
                        sql: Some(tokio::fs::read_to_string(sql_path).await?),
                    }))
                } else {
                    Err(OxyError::AgentError(format!(
                        "Unsupported SQL source type for path: {}",
                        &document.id
                    )))
                }
            }
            _ => {
                self.resolve_tool(execution_context, &document.id, true)
                    .await
            }
        }
    }

    async fn resolve_routes(
        &self,
        execution_context: &ExecutionContext,
        agent_name: &str,
        model: &Model,
        routing_agent: &RoutingAgent,
        query: &str,
    ) -> Result<Vec<ToolType>, OxyError> {
        let mut resolved_routes = vec![];
        let output = RetrievalExecutable::new()
            .execute(
                execution_context,
                RetrievalInput {
                    query: query.to_string(),
                    db_config: routing_agent.db_config.clone(),
                    db_name: format!("{}-routing", agent_name),
                    openai_config: model.clone(),
                    embedding_config: routing_agent.embedding_config.clone(),
                },
            )
            .await?;
        for document in output.to_documents() {
            if let Ok(tool) = self.resolve_document(execution_context, &document).await {
                resolved_routes.push(tool);
            }
        }
        Ok(resolved_routes)
    }
}

#[async_trait::async_trait]
impl Executable<RoutingAgentInput> for RoutingAgentExecutable {
    type Response = OutputContainer;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: RoutingAgentInput,
    ) -> Result<Self::Response, OxyError> {
        let RoutingAgentInput {
            agent_name,
            model,
            routing_agent,
            prompt,
            memory,
        } = input;
        let model = execution_context.config.resolve_model(&model)?;
        let tool_configs = self
            .resolve_routes(
                execution_context,
                &agent_name,
                model,
                &routing_agent,
                &prompt,
            )
            .await?;
        let mut react_loop_executable = build_react_loop(
            agent_name.clone(),
            model,
            tool_configs,
            routing_agent.synthesize_results,
        )
        .await?;
        let one_shot_input = OneShotInput {
            system_instructions: routing_agent.system_instructions.clone(),
            user_input: Some(prompt),
            memory,
        };
        let outputs = match routing_agent.route_fallback {
            Some(fallback) => {
                let fallback_tool = self
                    .resolve_tool(execution_context, &fallback, false)
                    .await?;
                let fallback_route = FallbackAgent::new(&agent_name, model, fallback_tool).await?;
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

        Ok(OutputContainer::List(
            outputs.into_iter().map(|o| o.content.into()).collect(),
        ))
    }
}

async fn build_react_loop(
    agent_name: String,
    model: &Model,
    tool_configs: Vec<ToolType>,
    synthesize_results: bool,
) -> Result<impl Executable<OneShotInput, Response = Vec<OpenAIExecutableResponse>> + Clone, OxyError>
{
    let tools: Vec<ChatCompletionTool> =
        futures::future::join_all(tool_configs.iter().map(ChatCompletionTool::from_tool_async))
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
    let client = OpenAIClient::with_config(model.try_into()?);
    let deduplicated_tools = deduplicate_tools(tools)?;
    Ok(builder.memo(vec![]).executable(OpenAIExecutable::new(
        client,
        model.model_name().to_string(),
        deduplicated_tools,
        None,
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
        let candidate_name = format!("{}_{}", base_name, suffix);
        if !seen_names.contains(&candidate_name) {
            return candidate_name;
        }
        suffix += 1;
    }
}
