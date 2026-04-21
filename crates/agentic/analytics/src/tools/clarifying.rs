//! Executor for **clarifying**-state tools (`search_catalog`).

use agentic_core::tools::ToolError;
use serde_json::{Value, json};

use crate::catalog::Catalog;

use super::{emit_tool_error, emit_tool_input, emit_tool_output};

/// Execute a **clarifying** tool against `catalog`.
#[tracing::instrument(
    skip(catalog),
    fields(oxy.name = "analytics.tool", oxy.span_type = "analytics", tool = %name)
)]
pub fn execute_clarifying_tool(
    name: &str,
    params: Value,
    catalog: &dyn Catalog,
) -> Result<Value, ToolError> {
    emit_tool_input(name, &params);
    let result = execute_clarifying_tool_inner(name, params, catalog);
    match &result {
        Ok(v) => emit_tool_output(v),
        Err(e) => emit_tool_error(e),
    }
    result
}

fn execute_clarifying_tool_inner(
    name: &str,
    params: Value,
    catalog: &dyn Catalog,
) -> Result<Value, ToolError> {
    match name {
        "search_catalog" => {
            let queries: Vec<&str> = params["queries"]
                .as_array()
                .ok_or_else(|| ToolError::BadParams("missing 'queries' array".into()))?
                .iter()
                .filter_map(|v| v.as_str())
                .collect();
            let res = catalog.search_catalog(&queries);
            let metrics: Vec<Value> = res
                .metrics
                .iter()
                .map(|m| {
                    let mut obj = json!({
                        "name": m.name,
                        "description": m.description
                    });
                    if !m.metric_type.is_empty() {
                        obj["aggregation"] = json!(m.metric_type);
                    }
                    if let Some(expr) = &m.expr {
                        obj["formula"] = json!(expr);
                    }
                    obj
                })
                .collect();
            let dims: Vec<Value> = res
                .dimensions
                .iter()
                .map(|d| json!({ "name": d.name, "description": d.description, "type": d.data_type }))
                .collect();
            let mut result = json!({ "metrics": metrics, "dimensions": dims });
            if !metrics.is_empty() && metrics.iter().any(|m| m.get("aggregation").is_some()) {
                result["hint"] = json!(
                    "These are semantic measures. Use the exact 'name' field \
                     (e.g. 'orders.revenue') in your output — do NOT write raw \
                     SQL expressions like SUM(...). The aggregation is handled \
                     automatically by the semantic layer."
                );
            }
            Ok(result)
        }

        _ => Err(ToolError::UnknownTool(name.into())),
    }
}
