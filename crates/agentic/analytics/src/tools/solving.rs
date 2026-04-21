//! Executor for **solving**-state tools (`execute_preview`).

use agentic_connector::DatabaseConnector;
use agentic_core::tools::ToolError;
use serde_json::{Value, json};

use super::{cell_to_json, emit_tool_error, emit_tool_input, emit_tool_output};

/// Execute a **solving** tool.
///
/// `connector` is used by `execute_preview` to run the SQL with LIMIT 5.
#[tracing::instrument(
    skip(connector),
    fields(oxy.name = "analytics.tool", oxy.span_type = "analytics", tool = %name)
)]
pub async fn execute_solving_tool(
    name: &str,
    params: Value,
    connector: &dyn DatabaseConnector,
) -> Result<Value, ToolError> {
    emit_tool_input(name, &params);
    let result = execute_solving_tool_inner(name, params, connector).await;
    match &result {
        Ok(v) => emit_tool_output(v),
        Err(e) => emit_tool_error(e),
    }
    result
}

async fn execute_solving_tool_inner(
    name: &str,
    params: Value,
    connector: &dyn DatabaseConnector,
) -> Result<Value, ToolError> {
    match name {
        "execute_preview" => {
            let sql = params["sql"]
                .as_str()
                .ok_or_else(|| ToolError::BadParams("missing 'sql'".into()))?;

            // Wrap in a subquery so any inner LIMIT is overridden by the outer LIMIT 5.
            let preview_sql = format!("SELECT * FROM ({sql}) AS _preview LIMIT 5");

            match connector.execute_query(&preview_sql, 5).await {
                Ok(exec) => {
                    let rows: Vec<Vec<Value>> = exec
                        .result
                        .rows
                        .iter()
                        .map(|row| row.0.iter().map(cell_to_json).collect())
                        .collect();
                    Ok(json!({
                        "ok": true,
                        "columns": exec.result.columns,
                        "rows": rows,
                        "row_count": exec.result.rows.len()
                    }))
                }
                Err(e) => Ok(json!({
                    "ok": false,
                    "error": e.to_string()
                })),
            }
        }

        _ => Err(ToolError::UnknownTool(name.into())),
    }
}
