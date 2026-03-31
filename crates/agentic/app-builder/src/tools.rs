//! Tool definitions and executors for the app builder domain.

use serde_json::{Value, json};

use agentic_connector::DatabaseConnector;
use agentic_core::result::CellValue;
use agentic_core::tools::{ToolDef, ToolError};

use agentic_analytics::{Catalog, SemanticCatalog};

use regex::Regex;

// ── Tool definitions per state ────────────────────────────────────────────────

/// Tools available during the **clarifying** state.
pub fn clarifying_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "search_catalog",
            description: "Search the catalog for tables, metrics, and dimensions matching given query terms.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "queries": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Search terms to match against table names, metrics, and descriptions."
                    }
                },
                "required": ["queries"]
            }),
        },
        ToolDef {
            name: "preview_data",
            description: "Preview the first rows of a table.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "table": { "type": "string", "description": "Table name to preview" },
                    "limit": {
                        "anyOf": [{ "type": "integer" }, { "type": "null" }],
                        "description": "Max rows to return. Use null to default to 5."
                    }
                },
                "required": ["table", "limit"]
            }),
        },
    ]
}

/// Tools available during the **specifying** state.
pub fn specifying_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "get_column_values",
            description: "Get distinct values for a column (useful for Select controls).",
            parameters: json!({
                "type": "object",
                "properties": {
                    "table": { "type": "string" },
                    "column": { "type": "string" },
                    "limit": {
                        "anyOf": [{ "type": "integer" }, { "type": "null" }],
                        "description": "Max distinct values. Use null to default to 50."
                    }
                },
                "required": ["table", "column", "limit"]
            }),
        },
        ToolDef {
            name: "get_column_range",
            description: "Get min, max, and distinct count for a column.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "table": { "type": "string" },
                    "column": { "type": "string" }
                },
                "required": ["table", "column"]
            }),
        },
        ToolDef {
            name: "get_join_path",
            description: "Get the join path between two tables.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string" },
                    "to": { "type": "string" }
                },
                "required": ["from", "to"]
            }),
        },
        ToolDef {
            name: "count_rows",
            description: "Count rows in a table, optionally with a filter.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "table": { "type": "string" },
                    "filter": {
                        "anyOf": [{ "type": "string" }, { "type": "null" }],
                        "description": "WHERE clause without the WHERE keyword. Use null to count all rows."
                    }
                },
                "required": ["table", "filter"]
            }),
        },
    ]
}

/// Tools available during the **solving** state.
pub fn solving_tools() -> Vec<ToolDef> {
    vec![ToolDef {
        name: "execute_preview",
        description: "Execute a SQL query preview (LIMIT 5). Template refs like {{ controls.X | sqlquote }} are replaced with '__preview__'. Returns rows on success or {error: string} on failure.",
        parameters: json!({
            "type": "object",
            "properties": {
                "sql": { "type": "string", "description": "SQL query to preview" }
            },
            "required": ["sql"]
        }),
    }]
}

// ── Tool executors ────────────────────────────────────────────────────────────

/// Helper to convert a [`CellValue`] to a display string.
fn cell_to_string(c: &CellValue) -> String {
    match c {
        CellValue::Text(s) => s.clone(),
        CellValue::Number(n) => n.to_string(),
        CellValue::Null => "NULL".to_string(),
    }
}

/// Execute a **clarifying** tool (catalog-only, no connector needed).
pub fn execute_clarifying_tool(
    name: &str,
    params: Value,
    catalog: &SemanticCatalog,
) -> Result<Value, ToolError> {
    match name {
        "search_catalog" => {
            let queries: Vec<String> = params["queries"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let query_refs: Vec<&str> = queries.iter().map(String::as_str).collect();
            let results = catalog.search_catalog(&query_refs);
            Ok(json!({
                "metrics": results.metrics.iter().map(|m| json!({ "name": m.name, "description": m.description })).collect::<Vec<_>>(),
                "dimensions": results.dimensions.iter().map(|d| json!({ "name": d.name, "description": d.description, "type": d.data_type })).collect::<Vec<_>>()
            }))
        }
        _ => Err(ToolError::UnknownTool(format!(
            "unknown clarifying tool: '{name}'"
        ))),
    }
}

/// Execute a **clarifying** tool, optionally dispatching to the connector for
/// the `preview_data` tool.
pub async fn execute_clarifying_tool_with_connector(
    name: &str,
    params: Value,
    catalog: &SemanticCatalog,
    connector: &dyn DatabaseConnector,
) -> Result<Value, ToolError> {
    match name {
        "search_catalog" => execute_clarifying_tool(name, params, catalog),
        "preview_data" => {
            let table = params["table"].as_str().unwrap_or("").to_string();
            let limit = params["limit"].as_u64().unwrap_or(5);
            let sql = format!("SELECT * FROM {table} LIMIT {limit}");
            match connector.execute_query(&sql, limit).await {
                Ok(exec) => {
                    let rows: Vec<Value> = exec
                        .result
                        .rows
                        .iter()
                        .map(|row| {
                            let obj: serde_json::Map<String, Value> = exec
                                .result
                                .columns
                                .iter()
                                .zip(row.0.iter())
                                .map(|(col, val)| (col.clone(), json!(cell_to_string(val))))
                                .collect();
                            Value::Object(obj)
                        })
                        .collect();
                    Ok(json!({ "columns": exec.result.columns, "rows": rows }))
                }
                Err(e) => Ok(json!({ "error": e.to_string() })),
            }
        }
        _ => Err(ToolError::UnknownTool(format!(
            "unknown clarifying tool: '{name}'"
        ))),
    }
}

/// Execute a **specifying** tool.
pub async fn execute_specifying_tool(
    name: &str,
    params: Value,
    catalog: &SemanticCatalog,
    connector: &dyn DatabaseConnector,
) -> Result<Value, ToolError> {
    match name {
        "get_column_values" => {
            let table = params["table"].as_str().unwrap_or("").to_string();
            let column = params["column"].as_str().unwrap_or("").to_string();
            let limit = params["limit"].as_u64().unwrap_or(50);
            let sql =
                format!(r#"SELECT DISTINCT "{column}" FROM "{table}" ORDER BY 1 LIMIT {limit}"#);
            match connector.execute_query(&sql, limit).await {
                Ok(exec) => {
                    let values: Vec<Value> = exec
                        .result
                        .rows
                        .iter()
                        .filter_map(|row| row.0.first())
                        .map(|c| json!(cell_to_string(c)))
                        .collect();
                    Ok(json!({ "values": values }))
                }
                Err(e) => Ok(json!({ "error": e.to_string() })),
            }
        }
        "get_column_range" => {
            let table = params["table"].as_str().unwrap_or("").to_string();
            let column = params["column"].as_str().unwrap_or("").to_string();
            let sql = format!(
                r#"SELECT MIN("{column}"), MAX("{column}"), COUNT(DISTINCT "{column}") FROM "{table}""#
            );
            match connector.execute_query(&sql, 1).await {
                Ok(exec) => {
                    if let Some(row) = exec.result.rows.first() {
                        let cells = &row.0;
                        Ok(json!({
                            "min": cells.first().map(cell_to_string).unwrap_or_default(),
                            "max": cells.get(1).map(cell_to_string).unwrap_or_default(),
                            "distinct_count": cells.get(2).map(cell_to_string).unwrap_or_default()
                        }))
                    } else {
                        Ok(json!({ "error": "no rows returned" }))
                    }
                }
                Err(e) => Ok(json!({ "error": e.to_string() })),
            }
        }
        "get_join_path" => {
            let from = params["from"].as_str().unwrap_or("").to_string();
            let to = params["to"].as_str().unwrap_or("").to_string();
            match catalog.get_join_path(&from, &to) {
                Some(jp) => Ok(json!({ "path": jp.path, "join_type": jp.join_type })),
                None => Ok(
                    json!({ "path": null, "error": format!("no join path found between '{from}' and '{to}'") }),
                ),
            }
        }
        "count_rows" => {
            let table = params["table"].as_str().unwrap_or("").to_string();
            let filter = params["filter"].as_str();
            let sql = if let Some(f) = filter {
                format!(r#"SELECT COUNT(*) FROM "{table}" WHERE {f}"#)
            } else {
                format!(r#"SELECT COUNT(*) FROM "{table}""#)
            };
            match connector.execute_query(&sql, 1).await {
                Ok(exec) => {
                    let count = exec
                        .result
                        .rows
                        .first()
                        .and_then(|row| row.0.first())
                        .map(cell_to_string)
                        .unwrap_or_else(|| "0".to_string());
                    Ok(json!({ "count": count }))
                }
                Err(e) => Ok(json!({ "error": e.to_string() })),
            }
        }
        _ => Err(ToolError::UnknownTool(format!(
            "unknown specifying tool: '{name}'"
        ))),
    }
}

/// Execute a **solving** tool.
pub async fn execute_solving_tool(
    name: &str,
    params: Value,
    connector: &dyn DatabaseConnector,
) -> Result<Value, ToolError> {
    match name {
        "execute_preview" => {
            let sql = params["sql"].as_str().unwrap_or("").to_string();
            // Replace {{ controls.X | sqlquote }} with '__preview__'.
            let re = Regex::new(r"\{\{\s*controls\.\w+\s*\|\s*sqlquote\s*\}\}").unwrap();
            let substituted = re.replace_all(&sql, "'__preview__'").to_string();
            let preview_sql = format!("{} LIMIT 5", substituted.trim_end_matches(';'));
            match connector.execute_query(&preview_sql, 5).await {
                Ok(exec) => {
                    let rows: Vec<Value> = exec
                        .result
                        .rows
                        .iter()
                        .map(|row| {
                            let obj: serde_json::Map<String, Value> = exec
                                .result
                                .columns
                                .iter()
                                .zip(row.0.iter())
                                .map(|(col, val)| (col.clone(), json!(cell_to_string(val))))
                                .collect();
                            Value::Object(obj)
                        })
                        .collect();
                    Ok(json!({ "ok": true, "columns": exec.result.columns, "rows": rows }))
                }
                Err(e) => Ok(json!({ "ok": false, "error": e.to_string() })),
            }
        }
        _ => Err(ToolError::UnknownTool(format!(
            "unknown solving tool: '{name}'"
        ))),
    }
}
