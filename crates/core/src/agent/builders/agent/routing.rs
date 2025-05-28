use std::path::PathBuf;

use async_openai::types::ChatCompletionTool;

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
    config::model::{AgentTool, ExecuteSQLTool, Model, RoutingAgent, ToolType, WorkflowTool},
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::ExecutableBuilder,
        types::{Document, OutputContainer},
    },
    service::agent::Message,
    tools::{RetrievalExecutable, types::RetrievalInput},
};

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
        document: &Document,
    ) -> Result<ToolType, OxyError> {
        match document.id.as_str() {
            workflow_path if workflow_path.ends_with(".workflow.yml") => {
                let workflow = execution_context
                    .config
                    .resolve_workflow(workflow_path)
                    .await?;
                Ok(ToolType::Workflow(WorkflowTool {
                    name: workflow.name,
                    workflow_ref: workflow_path.to_string(),
                    variables: workflow.variables,
                    description: workflow.description,
                    output_task_ref: None,
                }))
            }
            agent_path if agent_path.ends_with(".agent.yml") => {
                let agent = execution_context.config.resolve_agent(agent_path).await?;
                Ok(ToolType::Agent(AgentTool {
                    name: agent.name,
                    agent_ref: agent_path.to_string(),
                    description: agent.description,
                }))
            }
            sql_path if sql_path.ends_with(".sql") => {
                if let Some(database_ref) = parse_sql_source_type(document.kind.as_str()) {
                    execution_context.config.resolve_database(&database_ref)?;
                    Ok(ToolType::ExecuteSQL(ExecuteSQLTool {
                        database: database_ref.to_string(),
                        description: document.content.to_string(),
                        dry_run_limit: None,
                        name: PathBuf::from(sql_path)
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        sql: Some(tokio::fs::read_to_string(sql_path).await?),
                    }))
                } else {
                    Err(OxyError::AgentError(format!(
                        "Unsupported SQL source type for path: {}",
                        &document.id
                    )))
                }
            }
            _ => Err(OxyError::AgentError(format!(
                "Unsupported tool type for path: {}",
                &document.id
            ))),
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
            if let Ok(tool) = self.resolve_tool(execution_context, &document).await {
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
        let outputs = react_loop_executable
            .execute(
                execution_context,
                OneShotInput {
                    system_instructions: routing_agent.system_instructions.clone(),
                    user_input: Some(prompt),
                    memory,
                },
            )
            .await?;

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
) -> Result<impl Executable<OneShotInput, Response = Vec<OpenAIExecutableResponse>>, OxyError> {
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
    Ok(builder.memo(vec![]).executable(OpenAIExecutable::new(
        client,
        model.model_name().to_string(),
        tools,
    )))
}
