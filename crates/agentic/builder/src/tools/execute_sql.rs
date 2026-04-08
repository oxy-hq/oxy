//! `execute_sql` tool — run a SQL query against a project database and return results.

use std::path::Path;
use std::time::Duration;

use agentic_core::tools::{ToolDef, ToolError};
use arrow::json::ArrayWriter;
use serde_json::{json, Value};

const ROW_LIMIT: u64 = 100;
const QUERY_TIMEOUT: Duration = Duration::from_secs(30);

pub fn execute_sql_def() -> ToolDef {
    ToolDef {
        name: "execute_sql",
        description: "Execute a SQL query against one of the project's configured databases and \
                      return the first 100 rows. Useful for verifying that SQL works correctly \
                      before proposing file changes. Use the optional 'database' parameter to \
                      specify which database to query by its name from config.yml (defaults to \
                      the first configured database). \
                      Returns {ok, database, columns, rows, row_count} on success or \
                      {ok: false, error} on failure.",
        parameters: json!({
            "type": "object",
            "properties": {
                "sql": {
                    "type": "string",
                    "description": "The SQL query to execute"
                },
                "database": {
                    "type": ["string", "null"],
                    "description": "Database name from config.yml. Null defaults to the first configured database."
                }
            },
            "required": ["sql", "database"],
            "additionalProperties": false
        }),
    }
}

pub async fn execute_execute_sql(
    workspace_root: &Path,
    params: &Value,
    secrets_manager: Option<&oxy::adapters::secrets::SecretsManager>,
) -> Result<Value, ToolError> {
    let sql = params["sql"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'sql'".into()))?;

    // Build ConfigManager from project root (same pattern as validate_project).
    let config_manager = oxy::config::ConfigBuilder::new()
        .with_workspace_path(workspace_root)
        .map_err(|e| ToolError::Execution(format!("config error: {e}")))?
        .build()
        .await
        .map_err(|e| ToolError::Execution(format!("config error: {e}")))?;

    // Determine which database to use.
    let db_name = if let Some(name) = params["database"].as_str() {
        name.to_string()
    } else {
        config_manager
            .get_config()
            .databases
            .first()
            .ok_or_else(|| ToolError::Execution("no databases configured in config.yml".into()))?
            .name
            .clone()
    };

    // Use provided secrets_manager (with DB fallback) or fall back to env-only.
    let owned_sm;
    let secrets_manager = match secrets_manager {
        Some(sm) => sm,
        None => {
            owned_sm = oxy::adapters::secrets::SecretsManager::from_environment()
                .map_err(|e| ToolError::Execution(format!("secrets manager error: {e}")))?;
            &owned_sm
        }
    };

    // Build a connector for the target database.
    let connector = oxy::connector::Connector::from_database(
        &db_name,
        &config_manager,
        secrets_manager,
        Some(ROW_LIMIT),
        None,
        None,
    )
    .await
    .map_err(|e| ToolError::Execution(e.to_string()))?;

    // Wrap in a subquery so the outer LIMIT applies regardless of the user's SQL.
    let preview_sql = format!("SELECT * FROM ({sql}) AS _oxy_preview LIMIT {ROW_LIMIT}");

    let (batches, schema) = tokio::time::timeout(
        QUERY_TIMEOUT,
        connector.run_query_with_limit(&preview_sql, Some(ROW_LIMIT)),
    )
    .await
    .map_err(|_| ToolError::Execution("query timed out after 30 seconds".into()))?
    .map_err(|e| ToolError::Execution(e.to_string()))?;

    let columns: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();

    // Serialize batches to a JSON array using Arrow's writer.
    let mut buf = Vec::new();
    {
        let mut writer = ArrayWriter::new(&mut buf);
        writer
            .write_batches(&batches.iter().collect::<Vec<_>>())
            .map_err(|e| ToolError::Execution(format!("result serialization error: {e}")))?;
        writer
            .finish()
            .map_err(|e| ToolError::Execution(format!("result serialization error: {e}")))?;
    }

    let rows: Value = serde_json::from_slice(&buf)
        .map_err(|e| ToolError::Execution(format!("result parsing error: {e}")))?;

    Ok(json!({
        "ok": true,
        "database": db_name,
        "columns": columns,
        "rows": rows,
        "row_count": total_rows,
    }))
}
