use std::path::PathBuf;

use oxy_semantic::Topic;

use oxy::{
    adapters::vector_store::parse_sql_source_type,
    config::model::{
        AgentTool, EmbeddingConfig, ExecuteSQLTool, IntegrationType, OmniQueryTool,
        SemanticQueryTool, ToolType, VectorDBConfig, WorkflowTool,
    },
    execute::{Executable, ExecutionContext, types::Document},
    tools::{RetrievalExecutable, types::RetrievalInput},
    utils::to_openai_function_name,
};
use oxy_shared::errors::OxyError;

pub struct RouteResolver;

impl RouteResolver {
    fn parse_omni_route(route: &str) -> Result<(String, String), OxyError> {
        if let Some((integration_name, topic)) = route.split_once("::") {
            Ok((integration_name.to_string(), topic.to_string()))
        } else {
            Err(OxyError::AgentError(
                "Invalid omni route format".to_string(),
            ))
        }
    }

    pub async fn resolve_tool(
        execution_context: &ExecutionContext,
        file_ref: &str,
        description: Option<&str>,
        is_verified: bool,
    ) -> Result<ToolType, OxyError> {
        if file_ref.contains("::") {
            Self::resolve_integration_route(execution_context, file_ref, description).await
        } else {
            Self::resolve_file_route(execution_context, file_ref, description, is_verified).await
        }
    }

    async fn resolve_integration_route(
        execution_context: &ExecutionContext,
        file_ref: &str,
        description: Option<&str>,
    ) -> Result<ToolType, OxyError> {
        let integration_name = file_ref
            .split("::")
            .next()
            .ok_or_else(|| {
                OxyError::AgentError(format!("Invalid integration route format: {}", file_ref))
            })?
            .to_string();

        let integration = execution_context
            .project
            .config_manager
            .get_integration_by_name(&integration_name)
            .ok_or_else(|| {
                OxyError::AgentError(format!("Integration '{}' not found", integration_name))
            })?;

        match &integration.integration_type {
            IntegrationType::Omni(_) => {
                let (integration_name, topic) = Self::parse_omni_route(file_ref)?;
                let tool_description = Self::resolve_description(
                    description,
                    &format!(
                        "Query {} topic from {} integration",
                        topic, integration_name
                    ),
                );

                Ok(ToolType::OmniQuery(OmniQueryTool {
                    name: format!(
                        "{}_query_{}",
                        integration_name.to_lowercase(),
                        topic.to_lowercase()
                    ),
                    description: tool_description,
                    topic,
                    integration: integration_name,
                }))
            }
        }
    }

    pub async fn resolve_file_route(
        execution_context: &ExecutionContext,
        file_ref: &str,
        description: Option<&str>,
        is_verified: bool,
    ) -> Result<ToolType, OxyError> {
        match file_ref {
            workflow_path
                if workflow_path.ends_with(".workflow.yml")
                    || workflow_path.ends_with(".automation.yml") =>
            {
                Self::resolve_workflow_tool(
                    execution_context,
                    workflow_path,
                    description,
                    is_verified,
                )
                .await
            }
            agent_path if agent_path.ends_with(".agent.yml") => {
                Self::resolve_agent_tool(execution_context, agent_path, description, is_verified)
                    .await
            }
            aw_path if aw_path.ends_with(".aw.yml") || aw_path.ends_with(".aw.yaml") => {
                Self::resolve_agentic_workflow_tool(
                    execution_context,
                    aw_path,
                    description,
                    is_verified,
                )
                .await
            }
            topic_path if topic_path.ends_with(".topic.yml") => {
                Self::resolve_topic_tool(execution_context, topic_path, description).await
            }
            _ => Err(OxyError::AgentError(format!(
                "Unsupported tool type for path: {}",
                file_ref
            ))),
        }
    }

    async fn resolve_workflow_tool(
        execution_context: &ExecutionContext,
        workflow_path: &str,
        description: Option<&str>,
        is_verified: bool,
    ) -> Result<ToolType, OxyError> {
        let workflow = execution_context
            .project
            .config_manager
            .resolve_workflow(workflow_path)
            .await?;

        let tool_description = Self::resolve_description(description, &workflow.description);

        Ok(ToolType::Workflow(WorkflowTool {
            name: to_openai_function_name(
                &PathBuf::from(workflow_path),
                &execution_context
                    .project
                    .config_manager
                    .project_path()
                    .to_path_buf(),
            )?,
            workflow_ref: workflow_path.to_string(),
            variables: workflow.variables,
            description: tool_description,
            output_task_ref: None,
            is_verified,
        }))
    }

    async fn resolve_agent_tool(
        execution_context: &ExecutionContext,
        agent_path: &str,
        description: Option<&str>,
        is_verified: bool,
    ) -> Result<ToolType, OxyError> {
        let agent = execution_context
            .project
            .config_manager
            .resolve_agent(agent_path)
            .await?;
        let tool_description = Self::resolve_description(description, &agent.description);

        Ok(ToolType::Agent(AgentTool {
            name: to_openai_function_name(
                &PathBuf::from(agent_path),
                &execution_context
                    .project
                    .config_manager
                    .project_path()
                    .to_path_buf(),
            )?,
            agent_ref: agent_path.to_string(),
            description: tool_description,
            variables: None,
            is_verified,
        }))
    }

    async fn resolve_agentic_workflow_tool(
        execution_context: &ExecutionContext,
        aw_path: &str,
        description: Option<&str>,
        is_verified: bool,
    ) -> Result<ToolType, OxyError> {
        let aw = execution_context
            .project
            .config_manager
            .resolve_agentic_workflow(aw_path)
            .await?;
        let tool_description = Self::resolve_description(description, &aw.start.start.description);

        Ok(ToolType::Agent(AgentTool {
            name: to_openai_function_name(
                &PathBuf::from(aw_path),
                &execution_context
                    .project
                    .config_manager
                    .project_path()
                    .to_path_buf(),
            )?,
            agent_ref: aw_path.to_string(),
            description: tool_description,
            variables: None,
            is_verified,
        }))
    }

    async fn resolve_topic_tool(
        execution_context: &ExecutionContext,
        topic_path: &str,
        description: Option<&str>,
    ) -> Result<ToolType, OxyError> {
        let topic_file_path = std::path::Path::new(topic_path);
        if !tokio::fs::try_exists(topic_file_path).await.map_err(|e| {
            OxyError::AgentError(format!(
                "Failed to check topic file existence {}: {}",
                topic_path, e
            ))
        })? {
            return Err(OxyError::AgentError(format!(
                "Topic file does not exist: {}",
                topic_path
            )));
        }

        let content = tokio::fs::read_to_string(topic_file_path)
            .await
            .map_err(|e| {
                OxyError::AgentError(format!("Failed to read topic file {}: {}", topic_path, e))
            })?;

        let topic: Topic = serde_yaml::from_str(&content).map_err(|e| {
            OxyError::AgentError(format!("Failed to parse topic file {}: {}", topic_path, e))
        })?;

        let tool_description = Self::resolve_description(description, &topic.description);

        Ok(ToolType::SemanticQuery(SemanticQueryTool {
            name: to_openai_function_name(
                &PathBuf::from(topic_path),
                &execution_context
                    .project
                    .config_manager
                    .project_path()
                    .to_path_buf(),
            )?,
            topic: Some(topic.name.clone()),
            description: tool_description,
            dry_run_limit: None,
            variables: None,
        }))
    }

    pub fn resolve_description(description: Option<&str>, fallback: &str) -> String {
        description
            .map(|desc| desc.to_string())
            .unwrap_or_else(|| fallback.to_string())
    }

    pub async fn resolve_document(
        execution_context: &ExecutionContext,
        document: &Document,
    ) -> Result<ToolType, OxyError> {
        if document.id.contains("::") {
            Self::resolve_tool(
                execution_context,
                &document.id,
                Some(&document.content),
                true,
            )
            .await
        } else {
            let config_manager = &execution_context.project.config_manager;
            let project_path = config_manager.project_path();
            match document.id.as_str() {
                sql_path if sql_path.ends_with(".sql") => {
                    if let Some(database_ref) = parse_sql_source_type(document.kind.as_str()) {
                        config_manager.resolve_database(&database_ref)?;
                        Ok(ToolType::ExecuteSQL(ExecuteSQLTool {
                            database: database_ref.to_string(),
                            description: document.content.to_string(),
                            dry_run_limit: None,
                            name: to_openai_function_name(
                                &PathBuf::from(sql_path),
                                &PathBuf::from(project_path),
                            )?,
                            variables: None,
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
                    Self::resolve_tool(
                        execution_context,
                        &document.id,
                        Some(&document.content),
                        true,
                    )
                    .await
                }
            }
        }
    }

    pub async fn resolve_routes(
        execution_context: &ExecutionContext,
        agent_name: &str,
        db_config: &VectorDBConfig,
        embedding_config: &EmbeddingConfig,
        api_url: &str,
        key_var: &str,
        query: &str,
    ) -> Result<Vec<ToolType>, OxyError> {
        let mut resolved_routes = vec![];

        let retrieval_config = oxy::config::model::RetrievalConfig {
            name: "routing".to_string(),
            description: "Routing agent retrieval".to_string(),
            src: vec![],
            api_url: api_url.to_string(),
            api_key: None,
            key_var: key_var.to_string(),
            embedding_config: embedding_config.clone(),
            db_config: db_config.clone(),
        };

        let output = RetrievalExecutable::new()
            .execute(
                execution_context,
                RetrievalInput {
                    query: query.to_string(),
                    agent_name: agent_name.to_string(),
                    retrieval_config,
                },
            )
            .await?;

        for document in output.to_documents() {
            match Self::resolve_document(execution_context, &document).await {
                Ok(tool) => resolved_routes.push(tool),
                Err(e) => {
                    tracing::warn!("Failed to resolve route document '{}': {}", document.id, e)
                }
            }
        }

        tracing::info!(
            "Resolved {} routes from vector search",
            resolved_routes.len()
        );

        Ok(resolved_routes)
    }
}
