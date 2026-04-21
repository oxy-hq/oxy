//! Database-lookup tool executors (`list_tables`, `describe_table`) with a
//! shared [`SchemaCache`] so introspection runs at most once per connector.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agentic_connector::{DatabaseConnector, SchemaInfo};
use agentic_core::tools::ToolError;
use serde_json::{Value, json};

use super::{cell_to_json, emit_tool_error, emit_tool_input, emit_tool_output};

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
    fields(oxy.name = "analytics.tool", oxy.span_type = "analytics", tool = %name)
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
