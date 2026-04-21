//! Executor for **interpreting**-state tools (`render_chart`) plus the chart
//! config / column-type validators used for immediate self-correction.

use std::sync::{Arc, Mutex};

use agentic_core::events::{Event, EventStream};
use agentic_core::tools::ToolError;
use serde_json::{Value, json};

use crate::events::AnalyticsEvent;
use crate::types::{ChartConfig, DisplayBlock};

use super::{emit_tool_error, emit_tool_input, emit_tool_output};

/// Validate that chart columns requiring numeric data (y-axis, pie value) actually
/// contain at least one numeric JSON value in the result rows.
///
/// This catches the case where `to_2d_array` stringified Arrow numeric columns so
/// that `SUM(revenue)` values arrive as `"42000.0"` (a JSON string) instead of
/// `42000.0` (a JSON number).  The LLM receives an actionable error it can use to
/// self-correct without a full back-edge retry.
///
/// Returns an empty list when rows is empty (no data to inspect) or all checked
/// columns contain at least one numeric value.
pub fn validate_chart_column_types(
    config: &ChartConfig,
    columns: &[String],
    rows: &[Vec<serde_json::Value>],
) -> Vec<String> {
    if rows.is_empty() {
        return vec![];
    }

    let numeric_fields: &[(&str, &Option<String>)] = match config.chart_type.as_str() {
        "line_chart" | "bar_chart" => &[("y", &config.y)],
        "pie_chart" => &[("value", &config.value)],
        _ => return vec![],
    };

    let mut errors = Vec::new();
    for (field, col_opt) in numeric_fields {
        let col_name = match col_opt {
            Some(c) => c.as_str(),
            None => continue,
        };
        let Some(idx) = columns.iter().position(|c| c == col_name) else {
            continue; // name errors are reported by validate_chart_config
        };
        let has_numeric = rows
            .iter()
            .any(|row| row.get(idx).map(|v| v.is_number()).unwrap_or(false));
        if !has_numeric {
            errors.push(format!(
                "`{field}` column '{col_name}' contains no numeric values; \
                 charts require numeric data for this axis. \
                 The column may have been stringified — ensure the query \
                 returns a numeric type (e.g. CAST(... AS DOUBLE))."
            ));
        }
    }
    errors
}

/// Validate a chart config against the columns actually present in a result set.
///
/// Returns a list of human-readable error strings, one per bad reference.
/// An empty list means the config is valid.
pub fn validate_chart_config(config: &ChartConfig, columns: &[String]) -> Vec<String> {
    let col_set: std::collections::HashSet<&str> = columns.iter().map(|s| s.as_str()).collect();
    let available = columns.join(", ");
    let mut errors = Vec::new();

    let check = |field: &str, col: &str| -> Option<String> {
        if !col_set.contains(col) {
            Some(format!(
                "`{field}` references column '{col}' which does not exist; available: [{available}]"
            ))
        } else {
            None
        }
    };

    match config.chart_type.as_str() {
        "line_chart" | "bar_chart" => {
            for (field, val) in [("x", &config.x), ("y", &config.y), ("series", &config.series)] {
                if let Some(col) = val {
                    errors.extend(check(field, col));
                }
            }
        }
        "pie_chart" => {
            for (field, val) in [("name", &config.name), ("value", &config.value)] {
                if let Some(col) = val {
                    errors.extend(check(field, col));
                }
            }
        }
        "table" => {}
        unknown => errors.push(format!(
            "unknown chart_type '{unknown}'; must be one of: line_chart, bar_chart, pie_chart, table"
        )),
    }

    errors
}

/// Execute an **interpreting** tool.
///
/// `render_chart` validates the LLM-supplied column mappings immediately
/// against `result_sets[result_index]`.  On success it emits a
/// [`ChartRendered`] domain event (so the frontend receives the chart
/// mid-stream) and appends a [`DisplayBlock`] to `valid_charts`.
/// On failure it returns `{ok: false, errors: [...]}` so the LLM can
/// self-correct within the same tool loop without a full back-edge retry.
///
/// `result_sets` is a slice of `(columns, rows)` pairs — one entry per
/// executed spec.  Single-result queries have exactly one entry.
///
/// [`ChartRendered`]: crate::events::AnalyticsEvent::ChartRendered
#[tracing::instrument(
    skip(event_tx, result_sets, valid_charts),
    fields(oxy.name = "analytics.tool", oxy.span_type = "analytics", tool = %name)
)]
pub async fn execute_interpreting_tool(
    name: &str,
    params: Value,
    event_tx: &Option<EventStream<AnalyticsEvent>>,
    result_sets: &[(Vec<String>, Vec<Vec<serde_json::Value>>)],
    valid_charts: &Arc<Mutex<Vec<DisplayBlock>>>,
) -> Result<Value, ToolError> {
    emit_tool_input(name, &params);
    let result =
        execute_interpreting_tool_inner(name, params, event_tx, result_sets, valid_charts).await;
    match &result {
        Ok(v) => emit_tool_output(v),
        Err(e) => emit_tool_error(e),
    }
    result
}

async fn execute_interpreting_tool_inner(
    name: &str,
    params: Value,
    event_tx: &Option<EventStream<AnalyticsEvent>>,
    result_sets: &[(Vec<String>, Vec<Vec<serde_json::Value>>)],
    valid_charts: &Arc<Mutex<Vec<DisplayBlock>>>,
) -> Result<Value, ToolError> {
    match name {
        "render_chart" => {
            let chart_type = params["chart_type"]
                .as_str()
                .ok_or_else(|| ToolError::BadParams("missing 'chart_type'".into()))?
                .to_string();

            // Select the result set the LLM wants to visualize (default: first).
            let result_index = params["result_index"].as_u64().unwrap_or(0) as usize;
            let (columns, rows_json) = result_sets.get(result_index).ok_or_else(|| {
                ToolError::BadParams(format!(
                    "result_index {result_index} is out of range; \
                     there are {} result set(s)",
                    result_sets.len()
                ))
            })?;

            let config = ChartConfig {
                chart_type,
                x: params["x"].as_str().map(str::to_string),
                y: params["y"].as_str().map(str::to_string),
                series: params["series"].as_str().map(str::to_string),
                name: params["name"].as_str().map(str::to_string),
                value: params["value"].as_str().map(str::to_string),
                title: params["title"].as_str().map(str::to_string),
                x_axis_label: params["x_axis_label"].as_str().map(str::to_string),
                y_axis_label: params["y_axis_label"].as_str().map(str::to_string),
            };

            // Validate column names first, then column types.
            // Both checks return actionable errors the LLM can use to self-correct
            // within the same tool loop without a full back-edge retry.
            let mut errors = validate_chart_config(&config, columns);
            if errors.is_empty() {
                errors = validate_chart_column_types(&config, columns, rows_json);
            }
            if !errors.is_empty() {
                return Ok(json!({ "ok": false, "errors": errors }));
            }

            // Valid — emit the chart event and record it for the caller.
            if let Some(tx) = event_tx {
                let _ = tx
                    .send(Event::Domain(AnalyticsEvent::ChartRendered {
                        config: config.clone(),
                        columns: columns.clone(),
                        rows: rows_json.clone(),
                    }))
                    .await;
            }

            if let Ok(mut charts) = valid_charts.lock() {
                charts.push(DisplayBlock {
                    config,
                    columns: columns.clone(),
                    rows: rows_json.clone(),
                });
            }

            Ok(json!({ "ok": true }))
        }
        _ => Err(ToolError::UnknownTool(name.into())),
    }
}
