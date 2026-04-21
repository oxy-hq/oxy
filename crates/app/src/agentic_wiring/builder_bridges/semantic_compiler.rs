//! `BuilderSemanticCompiler` implementation backed by agentic-workflow.

use std::sync::Arc;

use agentic_builder::semantic::{BuilderSemanticCompiler, SemanticCompilationResult};
use agentic_core::tools::ToolError;
use agentic_workflow::WorkspaceContext;
use async_trait::async_trait;

use crate::agentic_wiring::OxyProjectContext;

/// Bridges builder semantic compilation to the semantic pipeline via agentic-workflow.
pub struct OxyBuilderSemanticCompiler {
    project_ctx: Arc<OxyProjectContext>,
}

impl OxyBuilderSemanticCompiler {
    pub fn new(project_ctx: Arc<OxyProjectContext>) -> Self {
        Self { project_ctx }
    }
}

#[async_trait]
impl BuilderSemanticCompiler for OxyBuilderSemanticCompiler {
    async fn compile(
        &self,
        params: &serde_json::Value,
    ) -> Result<SemanticCompilationResult, ToolError> {
        let task: agentic_workflow::config::SemanticQueryConfig =
            serde_json::from_value(params.clone())
                .map_err(|e| ToolError::BadParams(format!("invalid semantic query params: {e}")))?;

        let scan_path = self.project_ctx.workspace_path();
        let databases = self.project_ctx.database_configs();

        let (sql, database_name) =
            agentic_workflow::semantic_bridge::resolve_and_compile(scan_path, &databases, &task)
                .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(SemanticCompilationResult { sql, database_name })
    }
}
