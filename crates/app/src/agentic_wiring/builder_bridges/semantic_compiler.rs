//! `BuilderSemanticCompiler` implementation backed by `agentic-semantic`.

use std::sync::Arc;

use agentic_automation::WorkspaceContext;
use agentic_builder::semantic::{BuilderSemanticCompiler, PreaggHandle, SemanticCompilationResult};
use agentic_core::result::QueryResult;
use agentic_core::tools::ToolError;
use agentic_semantic::compile::{CompiledQuery, PreaggSource, resolve_and_compile};
use async_trait::async_trait;

use crate::agentic_wiring::OxyProjectContext;

/// Bridges builder semantic compilation to `agentic_semantic::compile`.
///
/// Returns either a warehouse SQL string (routed through a database
/// connector by the tool) or a pre-aggregation rewrite plus a handle to the
/// rollup behind it (executed via in-process DuckDB through `execute_preagg`).

/// `agentic-builder` deliberately doesn't depend on `agentic-semantic`, so the
/// source travels through its opaque [`PreaggHandle`] as JSON and comes back
/// here to be decoded. Nothing between the two ends interprets it.
fn encode_handle(source: &PreaggSource) -> Result<PreaggHandle, ToolError> {
    serde_json::to_string(source)
        .map(PreaggHandle)
        .map_err(|e| ToolError::Execution(format!("could not encode the rollup handle: {e}")))
}

fn decode_handle(handle: &PreaggHandle) -> Result<PreaggSource, ToolError> {
    serde_json::from_str(&handle.0)
        .map_err(|e| ToolError::Execution(format!("could not decode the rollup handle: {e}")))
}
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

        // BACKLOG: the semantic scan directory is `context_root()`, which
        // serves the compiled boundary; the workspace root is the ide's answer.
        let scan_path = self.project_ctx.workspace_path().ok_or_else(|| {
            ToolError::Execution("semantic query: this node holds no workspace files".to_string())
        })?;
        let databases = self.project_ctx.database_configs();
        let preagg = self.project_ctx.preagg_context();

        let compiled = resolve_and_compile(scan_path, &databases, &task, preagg.as_ref(), None)
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        match compiled {
            CompiledQuery::Warehouse { sql, database_name } => {
                Ok(SemanticCompilationResult::Warehouse { sql, database_name })
            }
            CompiledQuery::Preaggregation {
                preagg_sql,
                source,
                warehouse_sql,
                warehouse_database,
            } => Ok(SemanticCompilationResult::Preaggregation {
                preagg_sql,
                handle: encode_handle(&source)?,
                warehouse_sql,
                warehouse_database,
            }),
        }
    }

    async fn execute_preagg(
        &self,
        preagg_sql: &str,
        handle: &PreaggHandle,
        sample_limit: u64,
    ) -> Result<QueryResult, ToolError> {
        let preagg_sql = preagg_sql.to_string();
        let source = decode_handle(handle)?;
        let (columns, rows, total_row_count) = tokio::task::spawn_blocking(move || {
            agentic_semantic::preagg::execute_preagg_sql_typed(&preagg_sql, &source, sample_limit)
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
