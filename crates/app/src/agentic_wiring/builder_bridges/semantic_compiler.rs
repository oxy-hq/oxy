//! `BuilderSemanticCompiler` implementation backed by `agentic-semantic`.

use std::path::Path;
use std::sync::Arc;

use agentic_automation::WorkspaceContext;
use agentic_builder::semantic::{BuilderSemanticCompiler, SemanticCompilationResult};
use agentic_core::result::QueryResult;
use agentic_core::tools::ToolError;
use agentic_semantic::compile::{CompiledQuery, resolve_and_compile};
use async_trait::async_trait;

use crate::agentic_wiring::OxyProjectContext;

/// Bridges builder semantic compilation to `agentic_semantic::compile`.
///
/// Returns either a warehouse SQL string (routed through a database
/// connector by the tool) or a pre-aggregation rewrite + Parquet path
/// (executed via in-process DuckDB through `execute_preagg`).
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
        let task: agentic_semantic::config::SemanticQueryConfig =
            serde_json::from_value(params.clone())
                .map_err(|e| ToolError::BadParams(format!("invalid semantic query params: {e}")))?;

        let scan_path = self.project_ctx.workspace_path();
        let databases = self.project_ctx.database_configs();
        let cache = self.project_ctx.refresh_key_cache();
        let renewal_threshold_secs = self.project_ctx.preagg_renewal_threshold_secs();

        let compiled = resolve_and_compile(
            scan_path,
            &databases,
            &task,
            cache,
            renewal_threshold_secs,
            None,
        )
        .map_err(|e| ToolError::Execution(e.to_string()))?;

        match compiled {
            CompiledQuery::Warehouse { sql, database_name } => {
                Ok(SemanticCompilationResult::Warehouse { sql, database_name })
            }
            CompiledQuery::Preaggregation {
                preagg_sql,
                parquet_path,
                warehouse_sql,
                warehouse_database,
            } => Ok(SemanticCompilationResult::Preaggregation {
                preagg_sql,
                parquet_path,
                warehouse_sql,
                warehouse_database,
            }),
        }
    }

    async fn execute_preagg(
        &self,
        preagg_sql: &str,
        parquet_path: &Path,
        sample_limit: u64,
    ) -> Result<QueryResult, ToolError> {
        let preagg_sql = preagg_sql.to_string();
        let parquet_path = parquet_path.to_path_buf();
        let (columns, rows, total_row_count) = tokio::task::spawn_blocking(move || {
            agentic_semantic::preagg::execute_preagg_sql_typed(
                &preagg_sql,
                &parquet_path,
                sample_limit,
            )
        })
        .await
        .map_err(|e| ToolError::Execution(format!("preagg task panicked: {e}")))?
        .map_err(|e| ToolError::Execution(e.to_string()))?;

        let truncated = (rows.len() as u64) < total_row_count;
        Ok(QueryResult {
            columns,
            rows,
            total_row_count,
            truncated,
        })
    }
}
