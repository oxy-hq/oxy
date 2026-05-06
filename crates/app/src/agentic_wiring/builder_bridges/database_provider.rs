//! `BuilderDatabaseProvider` implementation backed by the platform port.

use std::sync::Arc;

use agentic_builder::BuilderDatabaseProvider;
use agentic_connector::DatabaseConnector;
use agentic_core::tools::ToolError;
use agentic_workflow::WorkspaceContext;
use async_trait::async_trait;

use crate::agentic_wiring::OxyProjectContext;

/// Bridges builder database operations to the platform `ProjectContext`.
pub struct OxyBuilderDatabaseProvider {
    project_ctx: Arc<OxyProjectContext>,
}

impl OxyBuilderDatabaseProvider {
    pub fn new(project_ctx: Arc<OxyProjectContext>) -> Self {
        Self { project_ctx }
    }
}

#[async_trait]
impl BuilderDatabaseProvider for OxyBuilderDatabaseProvider {
    async fn list_databases(&self) -> Result<Vec<String>, ToolError> {
        Ok(self
            .project_ctx
            .database_configs()
            .into_iter()
            .map(|db| db.name)
            .collect())
    }

    async fn get_connector(
        &self,
        database_name: &str,
    ) -> Result<Arc<dyn DatabaseConnector>, ToolError> {
        self.project_ctx
            .build_connector_for(database_name)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))
    }
}
