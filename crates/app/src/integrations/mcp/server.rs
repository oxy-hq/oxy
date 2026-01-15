// Standard library imports
use std::path::PathBuf;

// External crate imports
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParam, CallToolResult, ListToolsResult, PaginatedRequestParam,
        ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
};
use uuid::Uuid;

// Internal crate imports
use oxy::adapters::{project::builder::ProjectBuilder, runs::RunsManager, secrets::SecretsManager};
use oxy_shared::errors::OxyError;

use super::connections::extract_connection_overrides;
use super::context::ToolExecutionContext;
use super::filters::extract_session_filters;
use super::tools::get_mcp_tools;
use super::types::OxyMcpServer;
use super::variables::extract_meta_variables;

// =============================================================================
// ServerHandler Implementation
// =============================================================================

impl ServerHandler for OxyMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Oxy is the Data Agent Platform that brings intelligence to your structured enterprise data. Answer, build, and automate anything.".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }

    async fn list_tools(
        &self,
        _: std::option::Option<PaginatedRequestParam>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, rmcp::ErrorData> {
        let tools = self
            .tools
            .values()
            .map(|oxy_tool| oxy_tool.tool.clone())
            .collect::<Vec<_>>();
        Ok(ListToolsResult {
            tools,
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        params: CallToolRequestParam,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.setup_working_directory()?;

        let tool_name = params.name.clone().to_string();
        let oxy_tool =
            self.tools
                .get(tool_name.as_str())
                .ok_or(rmcp::ErrorData::invalid_request(
                    format!("Tool {tool_name} not found"),
                    None,
                ))?;

        // Build execution context from request metadata
        let context = ToolExecutionContext::new()
            .with_session_filters(extract_session_filters(
                Some(&ctx.meta.0),
                &self.project_manager.config_manager,
            )?)
            .with_connection_overrides(extract_connection_overrides(
                Some(&ctx.meta.0),
                &self.project_manager.config_manager,
            )?)
            .with_meta_variables(extract_meta_variables(Some(&ctx.meta.0))?);

        // Execute tool using its executor
        let executor = oxy_tool.tool_type.executor();
        executor
            .execute(
                &self.project_manager,
                oxy_tool.name.clone(),
                params.arguments,
                context,
            )
            .await
    }
}

// =============================================================================
// OxyMcpServer Implementation
// =============================================================================

impl OxyMcpServer {
    /// Creates a new OxyMcpServer instance
    pub async fn new(project_path: PathBuf) -> Result<Self, OxyError> {
        let project_manager = ProjectBuilder::new(Uuid::nil())
            .with_project_path(&project_path)
            .await
            .map_err(|e| OxyError::from(anyhow::anyhow!("Failed to create config manager: {e}")))?
            .with_secrets_manager(SecretsManager::from_environment().map_err(|e| {
                OxyError::from(anyhow::anyhow!("Failed to create secrets manager: {e}"))
            })?)
            .with_runs_manager(
                RunsManager::default(uuid::Uuid::nil(), uuid::Uuid::nil())
                    .await
                    .map_err(|e| {
                        OxyError::from(anyhow::anyhow!("Failed to create runs manager: {e}"))
                    })?,
            )
            .try_with_intent_classifier()
            .await
            .build()
            .await
            .map_err(|e| OxyError::from(anyhow::anyhow!("Failed to create config manager: {e}")))?;

        let config_manager = project_manager.config_manager.clone();

        let tools = get_mcp_tools(config_manager).await?;

        Ok(Self {
            tools,
            project_manager,
        })
    }

    /// Sets up the working directory for tool execution
    pub fn setup_working_directory(&self) -> Result<(), rmcp::ErrorData> {
        let config = self.project_manager.config_manager.get_config();
        std::env::set_current_dir(&config.project_path).map_err(|e| {
            rmcp::ErrorData::internal_error(format!("Failed to set current directory: {e}"), None)
        })
    }
}
