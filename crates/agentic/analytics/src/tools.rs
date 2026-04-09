//! Tool definitions and executors for the analytics domain.
//!
//! Tools are thin wrappers around [`Catalog`] and [`DatabaseConnector`] — they
//! expose raw data and let the LLM reason.  No validation logic lives inside a tool.
//!
//! # Scoping
//!
//! | State | Tools |
//! |---|---|
//! | `triage` | `search_procedures` |
//! | `clarifying` | `search_catalog`, `list_tables`, `describe_table` |
//! | `specifying` | `search_catalog`, `sample_columns`, `get_join_path`, `list_tables`, `describe_table` |
//! | `solving` | `execute_preview` |
//! | `interpreting` | `render_chart` |
//!
//! `list_metrics` and `list_dimensions` were removed — `search_catalog`
//! returns both metrics and dimensions in a single call.
//! `get_metric_definition` was removed — `search_catalog` now includes the
//! formula/expression for each metric, making the separate lookup redundant.
//! `get_valid_dimensions` was removed — redundant with catalog context already
//! in the LLM prompt.  `get_column_range` was replaced by `sample_column` which
//! runs a live query instead of returning stale pre-computed catalog data.
//! `explain_plan` (stub) and `dry_run` (table-name substring check) were replaced
//! by `execute_preview` which runs the SQL with LIMIT 5 and returns real results.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use agentic_connector::{DatabaseConnector, SchemaInfo};
use agentic_core::events::{Event, EventStream};
use agentic_core::result::CellValue;
use agentic_core::tools::{ToolDef, ToolError};

use crate::events::AnalyticsEvent;
use crate::types::{ChartConfig, DisplayBlock, QuestionType};

use crate::catalog::Catalog;

// ── Shared tool description strings ──────────────────────────────────────────
//
// Kept as constants so that triage, clarifying, and specifying stages stay in
// sync without copy-paste drift.

const SEARCH_CATALOG_DESC: &str = "Batch-search the semantic catalog for measures AND dimensions in one call. \
     Use this to check whether the catalog has all the members needed to answer \
     the question before attempting a semantic shortcut. Returns \
     {metrics: [{name, description}], dimensions: [{name, description, type}]}.";

const SEARCH_PROCEDURES_DESC: &str = "Search for existing procedure YAML files that match a query. \
     Returns a list of {name, path, description} entries. \
     Call this FIRST with key terms from the user's question. \
     If any procedure directly answers the question, select it.";

const SAMPLE_COLUMNS_DESC: &str = "Batch-sample multiple columns in one call. For each column, returns up \
     to 20 distinct non-null values plus statistics (row_count, distinct_count, \
     min, max; also avg and stdev for numeric columns). Accepts semantic view \
     names and dimension names as well as raw database table/column names. \
     Use this to verify filter values, confirm exact formats, and choose \
     date granularity — all in a single round-trip instead of calling \
     sample_column multiple times.";

// ── Tool definitions per state ────────────────────────────────────────────────

/// Tools available during the **triage** sub-phase of Clarify.
///
/// Only `search_procedures` is exposed — triage must check for an existing
/// procedure before doing any schema discovery.
pub fn triage_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "search_procedures",
            description: SEARCH_PROCEDURES_DESC,
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search term matched against procedure names and descriptions"
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
        ToolDef {
            name: "search_catalog",
            description: SEARCH_CATALOG_DESC,
            parameters: json!({
                "type": "object",
                "properties": {
                    "queries": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Search terms matched against measure/dimension names and descriptions."
                    }
                },
                "required": ["queries"],
                "additionalProperties": false
            }),
        },
    ]
}

/// Tools available during the **clarifying** state.
///
/// When `has_semantic` is `true` the semantic layer covers the data model and
/// raw database introspection tools (`list_tables`, `describe_table`) are
/// excluded to avoid confusing the LLM with two competing schema views.
pub fn clarifying_tools(has_semantic: bool) -> Vec<ToolDef> {
    let mut tools = vec![
        ToolDef {
            name: "search_catalog",
            description: SEARCH_CATALOG_DESC,
            parameters: json!({
                "type": "object",
                "properties": {
                    "queries": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "One or more search terms. Each term is matched against metric names and descriptions. Use [\"\"] to list everything."
                    }
                },
                "required": ["queries"],
                "additionalProperties": false
            }),
        },
        ToolDef {
            name: "search_procedures",
            description: SEARCH_PROCEDURES_DESC,
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search term matched against procedure names and descriptions"
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
    ];
    if !has_semantic {
        tools.push(list_tables_tool_def());
        tools.push(describe_table_tool_def());
    }
    tools
}

/// Tools available during the **specifying** state.
///
/// Includes `search_catalog` so Specifying can discover metrics/dimensions
/// directly from the raw question without a prior Ground phase.
///
/// When `has_semantic` is `true`, raw database tools (`list_tables`,
/// `describe_table`) are excluded — same rationale as [`clarifying_tools`].
pub fn specifying_tools(has_semantic: bool) -> Vec<ToolDef> {
    let mut tools = vec![
        ToolDef {
            name: "search_catalog",
            description: SEARCH_CATALOG_DESC,
            parameters: json!({
                "type": "object",
                "properties": {
                    "queries": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "One or more search terms matched against metric names and descriptions. Use [\"\"] to list everything."
                    }
                },
                "required": ["queries"],
                "additionalProperties": false
            }),
        },
        ToolDef {
            name: "sample_columns",
            description: SAMPLE_COLUMNS_DESC,
            parameters: json!({
                "type": "object",
                "properties": {
                    "columns": {
                        "type": "array",
                        "description": "One or more columns to sample.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "table": {
                                    "type": "string",
                                    "description": "Semantic view name or database table name"
                                },
                                "column": {
                                    "type": "string",
                                    "description": "Dimension/measure name or database column name"
                                },
                                "search_term": {
                                    "type": ["string", "null"],
                                    "description": "Optional substring filter (LIKE '%term%'). Pass null when not searching."
                                }
                            },
                            "required": ["table", "column", "search_term"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["columns"],
                "additionalProperties": false
            }),
        },
    ];
    if !has_semantic {
        // Without a semantic layer, the LLM needs manual join discovery and
        // raw schema introspection tools.
        tools.push(ToolDef {
            name: "get_join_path",
            description:
                "Return the join path between two entities: path expression and join type.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "from_entity": {
                        "type": "string",
                        "description": "Source table or entity name"
                    },
                    "to_entity": {
                        "type": "string",
                        "description": "Target table or entity name"
                    }
                },
                "required": ["from_entity", "to_entity"],
                "additionalProperties": false
            }),
        });
        tools.push(list_tables_tool_def());
        tools.push(describe_table_tool_def());
    }
    tools
}

/// Tools available during the **solving** state.
pub fn solving_tools() -> Vec<ToolDef> {
    vec![ToolDef {
        name: "execute_preview",
        description: "Run a SQL query with a hard LIMIT 5 and return real columns and rows. \
                      Use this to verify joins and filters produce actual results before \
                      finalizing the SQL. Returns {ok, columns, rows, row_count} on success \
                      or {ok: false, error} on failure.",
        parameters: json!({
            "type": "object",
            "properties": {
                "sql": {
                    "type": "string",
                    "description": "The SQL query to preview"
                }
            },
            "required": ["sql"],
            "additionalProperties": false
        }),
    }]
}

/// Tools available during the **interpreting** state.
pub fn interpreting_tools() -> Vec<ToolDef> {
    vec![ToolDef {
        name: "render_chart",
        description: "Render a chart or table from the query result. \
                      The data is already available from the executed query — \
                      only specify the chart type and which columns to use. \
                      Column names must exactly match the columns in the result set. \
                      Returns {ok: true} on success or {ok: false, errors: [...]} when a \
                      column name is wrong — fix and retry immediately. \
                      The chart is streamed to the client immediately when this tool is called. \
                      You may call it multiple times to produce multiple charts. \
                      When multiple result sets are available, use `result_index` to select which \
                      one to visualise (0-based, default 0).",
        parameters: json!({
            "type": "object",
            "properties": {
                "chart_type": {
                    "type": "string",
                    "enum": ["line_chart", "bar_chart", "pie_chart", "table"],
                    "description": "Chart variant to render"
                },
                "x": {
                    "type": ["string", "null"],
                    "description": "Column name for the x-axis. Required for line_chart and bar_chart. Use null for pie_chart and table."
                },
                "y": {
                    "type": ["string", "null"],
                    "description": "Column name for the y-axis / metric. Required for line_chart and bar_chart. Use null for pie_chart and table."
                },
                "series": {
                    "type": ["string", "null"],
                    "description": "Optional grouping column name to split data into multiple series \
        (line_chart / bar_chart only). When set, the data is grouped by this column's \
        distinct values and each group becomes a separate line or bar series in the chart. \
        For example, if x='month', y='revenue', series='region', the chart renders one \
        line/bar per region. Use null when there is no grouping column or for pie_chart/table."
                },
                "name": {
                    "type": ["string", "null"],
                    "description": "Category column name. Required for pie_chart. Use null for other chart types."
                },
                "value": {
                    "type": ["string", "null"],
                    "description": "Value column name. Required for pie_chart. Use null for other chart types."
                },
                "x_axis_label": {
                    "type": ["string", "null"],
                    "description": "Human-readable x-axis label (include units, e.g. 'Date', 'Revenue (USD)'). Use null to omit."
                },
                "y_axis_label": {
                    "type": ["string", "null"],
                    "description": "Human-readable y-axis label (include units, e.g. 'Sales ($)', 'Count'). Use null to omit."
                },
                "result_index": {
                    "type": ["integer", "null"],
                    "description": "Which result set to visualise (0-based). Use null to default to the first result set."
                },
                "title": {
                    "type": ["string", "null"],
                    "description": "Optional chart title. Use null to omit."
                }
            },
            "required": ["chart_type", "x", "y", "series", "name", "value", "x_axis_label", "y_axis_label", "result_index", "title"],
            "additionalProperties": false
        }),
    }]
}

/// Derive a deterministic [`ChartConfig`] suggestion from the question type and
/// result columns.
///
/// Returns `None` for question types that do not benefit from a chart (e.g.
/// `SingleValue`, `GeneralInquiry`) or when there are fewer than two columns.
pub fn suggest_chart_config(
    question_type: &QuestionType,
    columns: &[String],
) -> Option<ChartConfig> {
    if columns.len() < 2 {
        return None;
    }
    match question_type {
        QuestionType::Trend => Some(ChartConfig {
            chart_type: "line_chart".to_string(),
            x: Some(columns[0].clone()),
            y: Some(columns[1].clone()),
            series: columns.get(2).cloned(),
            name: None,
            value: None,
            title: None,
            x_axis_label: None,
            y_axis_label: None,
        }),
        QuestionType::Comparison | QuestionType::Breakdown => Some(ChartConfig {
            chart_type: "bar_chart".to_string(),
            x: Some(columns[0].clone()),
            y: Some(columns[1].clone()),
            series: columns.get(2).cloned(),
            name: None,
            value: None,
            title: None,
            x_axis_label: None,
            y_axis_label: None,
        }),
        QuestionType::Distribution => Some(ChartConfig {
            chart_type: "bar_chart".to_string(),
            x: Some(columns[0].clone()),
            y: Some(columns[1].clone()),
            series: None,
            name: None,
            value: None,
            title: None,
            x_axis_label: None,
            y_axis_label: None,
        }),
        QuestionType::SingleValue | QuestionType::GeneralInquiry => None,
    }
}

// ── Tool executors (thin Catalog wrappers) ────────────────────────────────────

/// Emit a visible `tool.input` event on the current span.
fn emit_tool_input(name: &str, params: &Value) {
    let input = serde_json::to_string(params).unwrap_or_default();
    let truncated = truncate_str(&input, 2000);
    tracing::info!(
        name: "tool.input",
        is_visible = true,
        tool_name = %name,
        input = %truncated,
    );
}

/// Emit a visible `tool.output` event on the current span.
fn emit_tool_output(output: &Value) {
    let text = serde_json::to_string(output).unwrap_or_default();
    let truncated = truncate_str(&text, 4000);
    tracing::info!(
        name: "tool.output",
        is_visible = true,
        output = %truncated,
    );
}

/// Emit a visible `tool.error` event on the current span.
fn emit_tool_error(err: &ToolError) {
    tracing::info!(
        name: "tool.output",
        is_visible = true,
        status = "error",
        error = %err,
    );
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}… ({} chars total)", &s[..max], s.len())
    }
}

/// Execute a **clarifying** tool against `catalog`.
#[tracing::instrument(
    skip(catalog),
    fields(otel.name = "analytics.tool", oxy.span_type = "analytics", tool = %name)
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

/// Sample a single column: resolve via catalog, query the database for distinct
/// values and statistics.  Used by `sample_columns` (batch) below.
async fn sample_single_column(
    table_param: &str,
    column_param: &str,
    search_term: Option<&str>,
    catalog: &dyn Catalog,
    connector: &dyn DatabaseConnector,
) -> Result<Value, ToolError> {
    // Resolve semantic view/dimension names to physical table/column.
    let resolved = catalog.resolve_sample_target(table_param, column_param);

    // If the semantic layer has static samples, return them directly.
    // When a search_term is provided, filter the static samples.
    if let Some(ref target) = resolved
        && !target.static_samples.is_empty()
    {
        let values: Vec<Value> = target
            .static_samples
            .iter()
            .filter(|s| {
                search_term
                    .map(|term| s.to_lowercase().contains(&term.to_lowercase()))
                    .unwrap_or(true)
            })
            .map(|s| Value::String(s.clone()))
            .collect();
        return Ok(json!({
            "column": column_param,
            "table": table_param,
            "data_type": target.data_type.as_deref().unwrap_or("string"),
            "sample_values": values,
            "source": "semantic_layer",
            "hint": "These are pre-defined sample values from the semantic layer definition."
        }));
    }

    // When the semantic layer resolves the target, `column_expr` is
    // already a SQL expression (e.g. `"Datetime (Local)"` with its own
    // quoting).  Use it verbatim instead of wrapping in extra quotes.
    let (table_sql, col_sql) = match &resolved {
        Some(target) => {
            let t = format!("\"{}\"", target.table.replace('"', "\"\""));
            // column_expr from the semantic layer is a raw SQL
            // expression — use it as-is.
            (t, target.column_expr.clone())
        }
        None => (
            format!("\"{}\"", table_param.replace('"', "\"\"")),
            format!("\"{}\"", column_param.replace('"', "\"\"")),
        ),
    };

    let sample_sql = if let Some(term) = search_term {
        // Escape single quotes in the search term to prevent SQL injection.
        let escaped = term.replace('\'', "''");
        format!(
            "SELECT DISTINCT {col_sql} FROM {table_sql} WHERE {col_sql} IS NOT NULL AND CAST({col_sql} AS TEXT) LIKE '%{escaped}%' LIMIT 20"
        )
    } else {
        format!("SELECT DISTINCT {col_sql} FROM {table_sql} WHERE {col_sql} IS NOT NULL LIMIT 20")
    };
    let count_sql = format!("SELECT COUNT(*) FROM {table_sql}");

    let sample_res = connector
        .execute_query(&sample_sql, 20)
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;

    let values: Vec<Value> = sample_res
        .result
        .rows
        .iter()
        .filter_map(|row| row.0.first())
        .map(cell_to_json)
        .collect();

    let data_type = sample_res
        .summary
        .columns
        .first()
        .map(|c| c.name.clone())
        .unwrap_or_default();

    // Detect column kind from sample values.
    // A value looks like a date if it starts with four digits followed by '-'.
    let is_date_col = values.iter().any(|v| {
        v.as_str()
            .map(|s| {
                s.len() >= 5
                    && s[..4].chars().all(|c| c.is_ascii_digit())
                    && s.as_bytes()[4] == b'-'
            })
            .unwrap_or(false)
    });
    // Numeric: all sampled values are JSON numbers (not dates).
    let is_numeric_col = !is_date_col && !values.is_empty() && values.iter().all(|v| v.is_number());

    // Helper to extract a u64 from a cell.
    let cell_u64 = |cell: &CellValue| -> Option<u64> {
        match cell {
            CellValue::Number(n) => Some(*n as u64),
            CellValue::Text(s) => s.parse().ok(),
            CellValue::Null => None,
        }
    };
    // Helper to extract a f64 from a cell.
    let cell_f64 = |cell: &CellValue| -> Option<f64> {
        match cell {
            CellValue::Number(n) => Some(*n),
            CellValue::Text(s) => s.parse().ok(),
            CellValue::Null => None,
        }
    };
    // Helper to extract a string from a cell.
    let cell_str = |cell: &CellValue| -> Option<String> {
        match cell {
            CellValue::Text(s) => Some(s.clone()),
            CellValue::Number(n) => Some(n.to_string()),
            CellValue::Null => None,
        }
    };

    let base = json!({ "column": column_param, "table": table_param });

    if is_numeric_col {
        // Numeric columns: fetch count, distinct count, min, max, avg, stdev in one query.
        let stats_sql = format!(
            "SELECT COUNT(*), COUNT(DISTINCT {col_sql}), MIN({col_sql}), MAX({col_sql}), \
             AVG({col_sql}), STDDEV_POP({col_sql}) FROM {table_sql}"
        );
        if let Ok(stats_res) = connector.execute_query(&stats_sql, 1).await
            && let Some(row) = stats_res.result.rows.first()
        {
            let row_count = row.0.first().and_then(&cell_u64).unwrap_or(0);
            let distinct_count = row.0.get(1).and_then(&cell_u64);
            let min_val = row.0.get(2).and_then(&cell_f64);
            let max_val = row.0.get(3).and_then(&cell_f64);
            let avg_val = row.0.get(4).and_then(&cell_f64);
            let stdev_val = row.0.get(5).and_then(cell_f64);
            let mut r = base;
            r["data_type"] = json!(data_type);
            r["sample_values"] = json!(values);
            r["row_count"] = json!(row_count);
            r["distinct_count"] = json!(distinct_count);
            r["min"] = json!(min_val);
            r["max"] = json!(max_val);
            r["avg"] = json!(avg_val);
            r["stdev"] = json!(stdev_val);
            return Ok(r);
        }
    } else {
        // Date and text columns: fetch count, distinct count, min, max.
        let stats_sql = format!(
            "SELECT COUNT(*), COUNT(DISTINCT {col_sql}), MIN({col_sql}), MAX({col_sql}) \
             FROM {table_sql}"
        );
        if let Ok(stats_res) = connector.execute_query(&stats_sql, 1).await
            && let Some(row) = stats_res.result.rows.first()
        {
            let row_count = row.0.first().and_then(&cell_u64).unwrap_or(0);
            let distinct_count = row.0.get(1).and_then(cell_u64);
            let min_val = row.0.get(2).and_then(&cell_str);
            let max_val = row.0.get(3).and_then(cell_str);
            let mut r = base;
            r["data_type"] = json!(data_type);
            r["sample_values"] = json!(values);
            r["row_count"] = json!(row_count);
            r["distinct_count"] = json!(distinct_count);
            r["min"] = json!(min_val);
            r["max"] = json!(max_val);
            return Ok(r);
        }
    }

    // Fallback: just return sample values with basic count.
    let count_res = connector
        .execute_query(&count_sql, 1)
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    let row_count: u64 = count_res
        .result
        .rows
        .first()
        .and_then(|row| row.0.first())
        .and_then(|cell| match cell {
            CellValue::Number(n) => Some(*n as u64),
            CellValue::Text(s) => s.parse().ok(),
            CellValue::Null => None,
        })
        .unwrap_or(0);
    let mut r = base;
    r["data_type"] = json!(data_type);
    r["sample_values"] = json!(values);
    r["row_count"] = json!(row_count);
    Ok(r)
}

/// Execute a **specifying** tool.
///
/// `catalog` is used for `get_join_path` lookups.
/// `connector` is used by `sample_columns` to run live queries.
#[tracing::instrument(
    skip(catalog, connector),
    fields(otel.name = "analytics.tool", oxy.span_type = "analytics", tool = %name)
)]
pub async fn execute_specifying_tool(
    name: &str,
    params: Value,
    catalog: &dyn Catalog,
    connector: &dyn DatabaseConnector,
) -> Result<Value, ToolError> {
    emit_tool_input(name, &params);
    let result = execute_specifying_tool_inner(name, params, catalog, connector).await;
    match &result {
        Ok(v) => emit_tool_output(v),
        Err(e) => emit_tool_error(e),
    }
    result
}

async fn execute_specifying_tool_inner(
    name: &str,
    params: Value,
    catalog: &dyn Catalog,
    connector: &dyn DatabaseConnector,
) -> Result<Value, ToolError> {
    match name {
        "get_join_path" => {
            let from = params["from_entity"]
                .as_str()
                .ok_or_else(|| ToolError::BadParams("missing 'from_entity'".into()))?;
            let to = params["to_entity"]
                .as_str()
                .ok_or_else(|| ToolError::BadParams("missing 'to_entity'".into()))?;
            match catalog.get_join_path(from, to) {
                Some(jp) => Ok(json!({ "path": jp.path, "join_type": jp.join_type })),
                None => {
                    let available = catalog.table_names().join(", ");
                    Err(ToolError::Execution(format!(
                        "no registered join path between '{from}' and '{to}'. \
                         You can still join these tables: use a bare column name if both tables \
                         share the same name (e.g. `Date`), or a \
                         `left_table.left_col = right_table.right_col` expression when the \
                         column names differ (e.g. `macro.Date = strength.workout_date`). \
                         Inspect both tables' columns to find the right key. \
                         Available tables: [{available}]."
                    )))
                }
            }
        }

        "sample_columns" => {
            let columns = params["columns"]
                .as_array()
                .ok_or_else(|| ToolError::BadParams("missing 'columns' array".into()))?;
            if columns.is_empty() {
                return Err(ToolError::BadParams(
                    "'columns' array must not be empty".into(),
                ));
            }

            // Collect validated column specs before spawning futures.
            let specs: Vec<(&str, &str, Option<&str>)> = columns
                .iter()
                .map(|entry| {
                    let table = entry["table"].as_str().ok_or_else(|| {
                        ToolError::BadParams("each column entry requires 'table'".into())
                    })?;
                    let column = entry["column"].as_str().ok_or_else(|| {
                        ToolError::BadParams("each column entry requires 'column'".into())
                    })?;
                    let search_term = entry
                        .get("search_term")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty());
                    Ok((table, column, search_term))
                })
                .collect::<Result<Vec<_>, ToolError>>()?;

            // Run all column samples concurrently.
            let futures: Vec<_> = specs
                .iter()
                .map(|(table, column, search_term)| {
                    sample_single_column(table, column, *search_term, catalog, connector)
                })
                .collect();
            let results = futures::future::join_all(futures).await;

            // Collect results — individual column errors become inline error objects
            // rather than failing the whole batch.
            let results_json: Vec<Value> = results
                .into_iter()
                .enumerate()
                .map(|(i, r)| match r {
                    Ok(v) => v,
                    Err(e) => json!({
                        "table": specs[i].0,
                        "column": specs[i].1,
                        "error": e.to_string()
                    }),
                })
                .collect();

            Ok(json!({ "results": results_json }))
        }

        _ => Err(ToolError::UnknownTool(name.into())),
    }
}

/// Execute a **solving** tool.
///
/// `connector` is used by `execute_preview` to run the SQL with LIMIT 5.
#[tracing::instrument(
    skip(connector),
    fields(otel.name = "analytics.tool", oxy.span_type = "analytics", tool = %name)
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

// ── Database lookup tools (lazy schema introspection) ─────────────────────────

/// Schema cache shared across tool calls within a solver session.
///
/// Populated lazily on first `list_tables` or `describe_table` call per
/// connector.  Avoids re-running `introspect_schema()` on every tool call.
pub type SchemaCache = Arc<Mutex<HashMap<String, SchemaInfo>>>;

/// Create a new empty schema cache.
pub fn new_schema_cache() -> SchemaCache {
    Arc::new(Mutex::new(HashMap::new()))
}

fn list_tables_tool_def() -> ToolDef {
    ToolDef {
        name: "list_tables",
        description: "List all tables available in the connected database(s). \
                      Use this when the semantic layer doesn't cover the data \
                      the user is asking about. Returns {tables: [{name, database}]}.",
        parameters: json!({
            "type": "object",
            "properties": {
                "database": {
                    "type": ["string", "null"],
                    "description": "Specific database/connector name. Use null to list from all databases."
                }
            },
            "required": ["database"],
            "additionalProperties": false
        }),
    }
}

fn describe_table_tool_def() -> ToolDef {
    ToolDef {
        name: "describe_table",
        description: "Get column names, data types, and sample values for a database table. \
                      Use this to understand table structure when the semantic layer doesn't \
                      have the information needed. \
                      Returns {table, columns: [{name, data_type, sample_values}]}.",
        parameters: json!({
            "type": "object",
            "properties": {
                "table": {
                    "type": "string",
                    "description": "Table name to describe"
                },
                "database": {
                    "type": ["string", "null"],
                    "description": "Connector name if multiple databases are configured. Use null for the default database."
                }
            },
            "required": ["table", "database"],
            "additionalProperties": false
        }),
    }
}

/// Resolve a connector by optional database name, falling back to the default.
fn resolve_connector<'a>(
    database: Option<&str>,
    connectors: &'a HashMap<String, Arc<dyn DatabaseConnector>>,
    default_connector: &str,
) -> Result<(String, &'a Arc<dyn DatabaseConnector>), ToolError> {
    let db_name = database.unwrap_or(default_connector);
    // Try exact match first, then case-insensitive.
    if let Some(conn) = connectors.get(db_name) {
        return Ok((db_name.to_string(), conn));
    }
    let lower = db_name.to_lowercase();
    for (name, conn) in connectors {
        if name.to_lowercase() == lower {
            return Ok((name.clone(), conn));
        }
    }
    let available: Vec<&str> = connectors.keys().map(|s| s.as_str()).collect();
    Err(ToolError::Execution(format!(
        "database '{db_name}' not found; available: [{}]",
        available.join(", ")
    )))
}

/// Ensure the schema for `connector_name` is cached, then return it.
fn cached_schema(
    connector_name: &str,
    connector: &dyn DatabaseConnector,
    cache: &SchemaCache,
) -> Result<SchemaInfo, ToolError> {
    {
        let guard = cache.lock().unwrap();
        if let Some(info) = guard.get(connector_name) {
            return Ok(info.clone());
        }
    }
    let info = connector
        .introspect_schema()
        .map_err(|e| ToolError::Execution(format!("schema introspection failed: {e}")))?;
    {
        let mut guard = cache.lock().unwrap();
        guard.insert(connector_name.to_string(), info.clone());
    }
    Ok(info)
}

/// Execute a **database lookup** tool (`list_tables` or `describe_table`).
///
/// These tools provide lazy, on-demand access to raw database schema when the
/// semantic layer doesn't cover the user's question.
#[tracing::instrument(
    skip(connectors, cache),
    fields(otel.name = "analytics.tool", oxy.span_type = "analytics", tool = %name)
)]
pub async fn execute_database_lookup_tool(
    name: &str,
    params: Value,
    connectors: &HashMap<String, Arc<dyn DatabaseConnector>>,
    default_connector: &str,
    cache: &SchemaCache,
) -> Result<Value, ToolError> {
    emit_tool_input(name, &params);
    let result =
        execute_database_lookup_tool_inner(name, params, connectors, default_connector, cache)
            .await;
    match &result {
        Ok(v) => emit_tool_output(v),
        Err(e) => emit_tool_error(e),
    }
    result
}

async fn execute_database_lookup_tool_inner(
    name: &str,
    params: Value,
    connectors: &HashMap<String, Arc<dyn DatabaseConnector>>,
    default_connector: &str,
    cache: &SchemaCache,
) -> Result<Value, ToolError> {
    match name {
        "list_tables" => {
            let database = params["database"].as_str();
            // If a specific database is requested, list only that one.
            // Otherwise, list from all connectors.
            let mut tables: Vec<Value> = Vec::new();
            if let Some(db) = database {
                let (db_name, conn) = resolve_connector(Some(db), connectors, default_connector)?;
                let info = cached_schema(&db_name, conn.as_ref(), cache)?;
                for t in &info.tables {
                    tables.push(json!({ "name": t.name, "database": &db_name }));
                }
            } else {
                for (db_name, conn) in connectors {
                    let info = cached_schema(db_name, conn.as_ref(), cache)?;
                    for t in &info.tables {
                        tables.push(json!({ "name": t.name, "database": db_name }));
                    }
                }
            }
            Ok(json!({ "tables": tables }))
        }

        "describe_table" => {
            let table = params["table"]
                .as_str()
                .ok_or_else(|| ToolError::BadParams("missing 'table'".into()))?;
            let database = params["database"].as_str();
            let (db_name, conn) = resolve_connector(database, connectors, default_connector)?;
            let info = cached_schema(&db_name, conn.as_ref(), cache)?;

            // Find the table (case-insensitive).
            let table_lower = table.to_lowercase();
            let table_info = info
                .tables
                .iter()
                .find(|t| t.name.to_lowercase() == table_lower)
                .ok_or_else(|| {
                    let available: Vec<&str> =
                        info.tables.iter().map(|t| t.name.as_str()).collect();
                    ToolError::Execution(format!(
                        "table '{table}' not found in database '{db_name}'; \
                         available: [{}]",
                        available.join(", ")
                    ))
                })?;

            let columns: Vec<Value> = table_info
                .columns
                .iter()
                .map(|col| {
                    let samples: Vec<Value> = col.sample_values.iter().map(cell_to_json).collect();
                    json!({
                        "name": col.name,
                        "data_type": col.data_type,
                        "sample_values": samples,
                    })
                })
                .collect();

            Ok(json!({
                "table": table_info.name,
                "database": db_name,
                "columns": columns,
            }))
        }

        _ => Err(ToolError::UnknownTool(name.into())),
    }
}

fn cell_to_json(cell: &CellValue) -> Value {
    match cell {
        CellValue::Text(s) => Value::String(s.clone()),
        CellValue::Number(n) => json!(n),
        CellValue::Null => Value::Null,
    }
}

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
    fields(otel.name = "analytics.tool", oxy.span_type = "analytics", tool = %name)
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SchemaCatalog;
    use agentic_llm::validate_openai_strict_schema;

    // ── OpenAI strict-mode compliance ─────────────────────────────────────────

    /// Every tool schema is sent to OpenAI with `"strict": true`.
    /// That mode requires every key in `properties` to also appear in
    /// `required`.  This test catches violations at compile time rather than
    /// at runtime when the HTTP call fails with an opaque 400.
    #[test]
    fn all_tool_schemas_are_openai_strict_compatible() {
        let all: Vec<ToolDef> = clarifying_tools(false)
            .into_iter()
            .chain(specifying_tools(false))
            .chain(solving_tools())
            .chain(interpreting_tools())
            .collect();

        for tool in &all {
            let violations = validate_openai_strict_schema(&tool.parameters, tool.name);
            assert!(
                violations.is_empty(),
                "tool '{}' violates OpenAI strict mode:\n  {}",
                tool.name,
                violations.join("\n  ")
            );
        }
    }

    fn make_catalog() -> SchemaCatalog {
        SchemaCatalog::new()
            .add_table("orders", &["order_id", "customer_id", "revenue", "date"])
            .add_table("customers", &["customer_id", "region", "name"])
            .add_join_key("orders", "customers", "customer_id")
    }

    // ── Tool scoping ──────────────────────────────────────────────────────────

    #[test]
    fn clarifying_does_not_include_solving_tools() {
        let tools = clarifying_tools(false);
        let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
        assert!(
            !names.contains(&"execute_preview"),
            "execute_preview must not appear in clarifying"
        );
        assert!(
            !names.contains(&"update_chart_config"),
            "update_chart_config must not appear in clarifying"
        );
    }

    #[test]
    fn solving_does_not_include_clarifying_tools() {
        let tools = solving_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
        assert!(
            !names.contains(&"search_catalog"),
            "search_catalog must not appear in solving"
        );
    }

    #[test]
    fn specifying_does_not_include_solving_tools() {
        let tools = specifying_tools(false);
        let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
        assert!(!names.contains(&"execute_preview"));
        assert!(!names.contains(&"update_chart_config"));
    }

    #[test]
    fn specifying_contains_expected_tools() {
        let names: Vec<&str> = specifying_tools(false).iter().map(|t| t.name).collect();
        assert!(names.contains(&"get_join_path"));
        assert!(names.contains(&"sample_columns"));
        // Catalog discovery tools moved from clarifying to specifying.
        assert!(names.contains(&"search_catalog"));
    }

    #[test]
    fn solving_contains_execute_preview() {
        let names: Vec<&str> = solving_tools().iter().map(|t| t.name).collect();
        assert!(names.contains(&"execute_preview"));
    }

    // ── Clarifying tool execution ─────────────────────────────────────────────

    #[test]
    fn search_catalog_finds_metrics_and_dimensions() {
        let cat = make_catalog();
        let result = execute_clarifying_tool(
            "search_catalog",
            serde_json::json!({ "queries": ["revenue"] }),
            &cat,
        )
        .unwrap();
        let metrics = result["metrics"].as_array().unwrap();
        assert!(
            !metrics.is_empty(),
            "revenue should match at least one metric"
        );
        assert!(
            metrics
                .iter()
                .any(|m| m["name"].as_str().unwrap_or("").contains("revenue"))
        );
        let dims = result["dimensions"].as_array().unwrap();
        assert!(!dims.is_empty(), "matched metric should have dimensions");
    }

    #[test]
    fn search_catalog_empty_query_returns_all() {
        let cat = make_catalog();
        let result = execute_clarifying_tool(
            "search_catalog",
            serde_json::json!({ "queries": [""] }),
            &cat,
        )
        .unwrap();
        let metrics = result["metrics"].as_array().unwrap();
        // orders has `revenue`; that's at least 1 metric
        assert!(!metrics.is_empty());
    }

    // ── Specifying tool execution ─────────────────────────────────────────────

    /// Noop connector used in specifying tests that call `get_join_path`
    /// (which never touches the connector).
    struct NoopConnector;

    #[async_trait::async_trait]
    impl DatabaseConnector for NoopConnector {
        fn dialect(&self) -> agentic_connector::SqlDialect {
            agentic_connector::SqlDialect::DuckDb
        }

        async fn execute_query(
            &self,
            _sql: &str,
            _limit: u64,
        ) -> Result<agentic_connector::ExecutionResult, agentic_connector::ConnectorError> {
            panic!("NoopConnector: execute_query must not be called in this test")
        }
    }

    #[tokio::test]
    async fn get_join_path_known_pair() {
        let cat = make_catalog();
        let conn = NoopConnector;
        let result = execute_specifying_tool(
            "get_join_path",
            serde_json::json!({ "from_entity": "orders", "to_entity": "customers" }),
            &cat,
            &conn,
        )
        .await
        .unwrap();
        assert!(result["path"].as_str().unwrap().contains("customer_id"));
        assert_eq!(result["join_type"], "INNER");
    }

    #[tokio::test]
    async fn get_join_path_unknown_pair_returns_error() {
        let cat = make_catalog();
        let conn = NoopConnector;
        let err = execute_specifying_tool(
            "get_join_path",
            serde_json::json!({ "from_entity": "orders", "to_entity": "products" }),
            &cat,
            &conn,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));
    }

    // ── Interpreting tool execution ───────────────────────────────────────────

    #[tokio::test]
    async fn render_chart_emits_event_and_returns_ok() {
        let cols = vec!["region".to_string(), "revenue".to_string()];
        let rows: Vec<Vec<serde_json::Value>> = vec![];
        let result_sets = vec![(cols, rows)];
        let valid_charts = Arc::new(Mutex::new(Vec::new()));
        let result = execute_interpreting_tool(
            "render_chart",
            serde_json::json!({
                "chart_type": "bar_chart",
                "x": "region",
                "y": "revenue",
                "series": null,
                "name": null,
                "value": null,
                "x_axis_label": "Region",
                "y_axis_label": "Revenue",
                "result_index": null,
                "title": "Revenue by Region"
            }),
            &None,
            &result_sets,
            &valid_charts,
        )
        .await
        .unwrap();
        assert_eq!(result["ok"], true);
    }

    // ── suggest_chart_config ─────────────────────────────────────────────────

    #[test]
    fn suggest_trend_produces_line_chart() {
        let cols = vec!["date".to_string(), "revenue".to_string()];
        let cfg = suggest_chart_config(&crate::types::QuestionType::Trend, &cols).unwrap();
        assert_eq!(cfg.chart_type, "line_chart");
        assert_eq!(cfg.x.as_deref(), Some("date"));
        assert_eq!(cfg.y.as_deref(), Some("revenue"));
    }

    #[test]
    fn suggest_breakdown_produces_bar_chart() {
        let cols = vec!["region".to_string(), "revenue".to_string()];
        let cfg = suggest_chart_config(&crate::types::QuestionType::Breakdown, &cols).unwrap();
        assert_eq!(cfg.chart_type, "bar_chart");
    }

    #[test]
    fn suggest_single_value_returns_none() {
        let cols = vec!["total".to_string()];
        assert!(suggest_chart_config(&crate::types::QuestionType::SingleValue, &cols).is_none());
    }

    #[test]
    fn suggest_fewer_than_two_columns_returns_none() {
        let cols = vec!["total".to_string()];
        assert!(suggest_chart_config(&crate::types::QuestionType::Breakdown, &cols).is_none());
    }

    // ── validate_chart_column_types ───────────────────────────────────────────

    #[test]
    fn column_type_check_passes_for_numeric_y() {
        let config = ChartConfig {
            chart_type: "bar_chart".to_string(),
            x: Some("region".to_string()),
            y: Some("revenue".to_string()),
            series: None,
            name: None,
            value: None,
            title: None,
            x_axis_label: None,
            y_axis_label: None,
        };
        let columns = vec!["region".to_string(), "revenue".to_string()];
        let rows = vec![vec![serde_json::json!("North"), serde_json::json!(42000.0)]];
        let errors = validate_chart_column_types(&config, &columns, &rows);
        assert!(
            errors.is_empty(),
            "numeric y column should pass: {errors:?}"
        );
    }

    #[test]
    fn column_type_check_fails_for_stringified_numeric_y() {
        // Regression: to_2d_array stringifies Arrow numeric columns; the chart
        // renderer receives "42000.0" (a JSON string) instead of 42000.0.
        let config = ChartConfig {
            chart_type: "bar_chart".to_string(),
            x: Some("region".to_string()),
            y: Some("revenue".to_string()),
            series: None,
            name: None,
            value: None,
            title: None,
            x_axis_label: None,
            y_axis_label: None,
        };
        let columns = vec!["region".to_string(), "revenue".to_string()];
        let rows = vec![
            // Both values are JSON strings — simulates the stringification bug.
            vec![serde_json::json!("North"), serde_json::json!("42000.0")],
        ];
        let errors = validate_chart_column_types(&config, &columns, &rows);
        assert!(
            !errors.is_empty(),
            "stringified y column should produce an error"
        );
        assert!(errors[0].contains("revenue"));
    }

    #[test]
    fn column_type_check_passes_for_pie_numeric_value() {
        let config = ChartConfig {
            chart_type: "pie_chart".to_string(),
            x: None,
            y: None,
            series: None,
            name: Some("category".to_string()),
            value: Some("share".to_string()),
            title: None,
            x_axis_label: None,
            y_axis_label: None,
        };
        let columns = vec!["category".to_string(), "share".to_string()];
        let rows = vec![vec![serde_json::json!("A"), serde_json::json!(0.4)]];
        assert!(validate_chart_column_types(&config, &columns, &rows).is_empty());
    }

    #[test]
    fn column_type_check_skipped_when_no_rows() {
        let config = ChartConfig {
            chart_type: "bar_chart".to_string(),
            x: Some("region".to_string()),
            y: Some("revenue".to_string()),
            series: None,
            name: None,
            value: None,
            title: None,
            x_axis_label: None,
            y_axis_label: None,
        };
        let columns = vec!["region".to_string(), "revenue".to_string()];
        // Empty result set — no rows to inspect, should not error.
        let errors = validate_chart_column_types(&config, &columns, &[]);
        assert!(errors.is_empty());
    }

    // ── Unknown tools return ToolError::UnknownTool ───────────────────────────

    #[test]
    fn unknown_tool_in_clarifying_returns_error() {
        let cat = make_catalog();
        let err = execute_clarifying_tool(
            "explain_plan",
            serde_json::json!({ "sql": "SELECT 1" }),
            &cat,
        )
        .unwrap_err();
        assert!(matches!(err, ToolError::UnknownTool(_)));
    }

    // ── Database lookup tools ─────────────────────────────────────────────────

    #[test]
    fn list_tables_tool_in_clarifying_and_specifying() {
        let clar_names: Vec<&str> = clarifying_tools(false).iter().map(|t| t.name).collect();
        let spec_names: Vec<&str> = specifying_tools(false).iter().map(|t| t.name).collect();
        assert!(clar_names.contains(&"list_tables"));
        assert!(spec_names.contains(&"list_tables"));
    }

    #[test]
    fn describe_table_tool_in_clarifying_and_specifying() {
        let clar_names: Vec<&str> = clarifying_tools(false).iter().map(|t| t.name).collect();
        let spec_names: Vec<&str> = specifying_tools(false).iter().map(|t| t.name).collect();
        assert!(clar_names.contains(&"describe_table"));
        assert!(spec_names.contains(&"describe_table"));
    }

    #[test]
    fn list_tables_not_in_solving() {
        let names: Vec<&str> = solving_tools().iter().map(|t| t.name).collect();
        assert!(!names.contains(&"list_tables"));
        assert!(!names.contains(&"describe_table"));
    }

    #[test]
    fn db_tools_excluded_when_has_semantic() {
        let clar = clarifying_tools(true);
        let spec = specifying_tools(true);
        let clar_names: Vec<&str> = clar.iter().map(|t| t.name).collect();
        let spec_names: Vec<&str> = spec.iter().map(|t| t.name).collect();
        assert!(!clar_names.contains(&"list_tables"));
        assert!(!clar_names.contains(&"describe_table"));
        assert!(!spec_names.contains(&"list_tables"));
        assert!(!spec_names.contains(&"describe_table"));
        // Core tools remain present.
        assert!(clar_names.contains(&"search_catalog"));
        assert!(spec_names.contains(&"sample_columns"));
    }

    /// Stub connector that returns a fixed schema for introspection.
    struct IntrospectableStub {
        schema: SchemaInfo,
        call_count: std::sync::atomic::AtomicUsize,
    }

    impl IntrospectableStub {
        fn new(schema: SchemaInfo) -> Self {
            Self {
                schema,
                call_count: std::sync::atomic::AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.call_count.load(std::sync::atomic::Ordering::Relaxed)
        }
    }

    #[async_trait::async_trait]
    impl DatabaseConnector for IntrospectableStub {
        fn dialect(&self) -> agentic_connector::SqlDialect {
            agentic_connector::SqlDialect::DuckDb
        }

        async fn execute_query(
            &self,
            _sql: &str,
            _limit: u64,
        ) -> Result<agentic_connector::ExecutionResult, agentic_connector::ConnectorError> {
            panic!("IntrospectableStub: execute_query must not be called")
        }

        fn introspect_schema(
            &self,
        ) -> Result<agentic_connector::SchemaInfo, agentic_connector::ConnectorError> {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(self.schema.clone())
        }
    }

    fn sample_schema() -> SchemaInfo {
        use agentic_connector::{SchemaColumnInfo, SchemaTableInfo};
        SchemaInfo {
            tables: vec![
                SchemaTableInfo {
                    name: "orders".to_string(),
                    columns: vec![
                        SchemaColumnInfo {
                            name: "order_id".to_string(),
                            data_type: "INTEGER".to_string(),
                            min: None,
                            max: None,
                            sample_values: vec![],
                        },
                        SchemaColumnInfo {
                            name: "revenue".to_string(),
                            data_type: "DOUBLE".to_string(),
                            min: None,
                            max: None,
                            sample_values: vec![CellValue::Number(100.0)],
                        },
                    ],
                },
                SchemaTableInfo {
                    name: "customers".to_string(),
                    columns: vec![SchemaColumnInfo {
                        name: "name".to_string(),
                        data_type: "VARCHAR".to_string(),
                        min: None,
                        max: None,
                        sample_values: vec![CellValue::Text("Alice".to_string())],
                    }],
                },
            ],
            join_keys: vec![],
        }
    }

    fn make_connectors(
        stub: Arc<IntrospectableStub>,
    ) -> HashMap<String, Arc<dyn DatabaseConnector>> {
        let mut map: HashMap<String, Arc<dyn DatabaseConnector>> = HashMap::new();
        map.insert("default".to_string(), stub);
        map
    }

    #[tokio::test]
    async fn list_tables_returns_table_names() {
        let stub = Arc::new(IntrospectableStub::new(sample_schema()));
        let connectors = make_connectors(stub);
        let cache = new_schema_cache();
        let result = execute_database_lookup_tool(
            "list_tables",
            json!({ "database": null }),
            &connectors,
            "default",
            &cache,
        )
        .await
        .unwrap();
        let tables = result["tables"].as_array().unwrap();
        assert_eq!(tables.len(), 2);
        let names: Vec<&str> = tables.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"orders"));
        assert!(names.contains(&"customers"));
    }

    #[tokio::test]
    async fn describe_table_returns_columns() {
        let stub = Arc::new(IntrospectableStub::new(sample_schema()));
        let connectors = make_connectors(stub);
        let cache = new_schema_cache();
        let result = execute_database_lookup_tool(
            "describe_table",
            json!({ "table": "orders", "database": null }),
            &connectors,
            "default",
            &cache,
        )
        .await
        .unwrap();
        assert_eq!(result["table"], "orders");
        let cols = result["columns"].as_array().unwrap();
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0]["name"], "order_id");
        assert_eq!(cols[0]["data_type"], "INTEGER");
        assert_eq!(cols[1]["name"], "revenue");
        // Check sample values are included.
        let samples = cols[1]["sample_values"].as_array().unwrap();
        assert!(!samples.is_empty());
    }

    #[tokio::test]
    async fn describe_table_unknown_returns_error() {
        let stub = Arc::new(IntrospectableStub::new(sample_schema()));
        let connectors = make_connectors(stub);
        let cache = new_schema_cache();
        let err = execute_database_lookup_tool(
            "describe_table",
            json!({ "table": "nonexistent", "database": null }),
            &connectors,
            "default",
            &cache,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));
    }

    #[tokio::test]
    async fn list_tables_caches_schema() {
        let stub = Arc::new(IntrospectableStub::new(sample_schema()));
        let connectors = make_connectors(Arc::clone(&stub));
        let cache = new_schema_cache();
        // First call populates cache.
        execute_database_lookup_tool(
            "list_tables",
            json!({ "database": null }),
            &connectors,
            "default",
            &cache,
        )
        .await
        .unwrap();
        assert_eq!(stub.calls(), 1);
        // Second call uses cache.
        execute_database_lookup_tool(
            "list_tables",
            json!({ "database": null }),
            &connectors,
            "default",
            &cache,
        )
        .await
        .unwrap();
        assert_eq!(
            stub.calls(),
            1,
            "introspect_schema should be called only once"
        );
    }

    #[tokio::test]
    async fn describe_table_case_insensitive() {
        let stub = Arc::new(IntrospectableStub::new(sample_schema()));
        let connectors = make_connectors(stub);
        let cache = new_schema_cache();
        let result = execute_database_lookup_tool(
            "describe_table",
            json!({ "table": "ORDERS", "database": null }),
            &connectors,
            "default",
            &cache,
        )
        .await
        .unwrap();
        assert_eq!(result["table"], "orders");
    }
}
