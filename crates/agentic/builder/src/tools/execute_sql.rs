//! `execute_sql` tool — run a SQL query against a project database and return results.

use std::time::Duration;

use agentic_core::tools::{ToolDef, ToolError};
use serde_json::{Value, json};

use crate::database::BuilderDatabaseProvider;

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
        ..Default::default()
    }
}

pub async fn execute_execute_sql(
    params: &Value,
    db_provider: &dyn BuilderDatabaseProvider,
) -> Result<Value, ToolError> {
    let sql = params["sql"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'sql'".into()))?;

    // Determine which database to use.
    let db_name = if let Some(name) = params["database"].as_str() {
        name.to_string()
    } else {
        let databases = db_provider.list_databases().await?;
        databases
            .into_iter()
            .next()
            .ok_or_else(|| ToolError::Execution("no databases configured in config.yml".into()))?
    };

    let connector = db_provider.get_connector(&db_name).await?;

    // Wrap in a subquery so the outer LIMIT applies regardless of the user's SQL.
    let preview_sql = format!("SELECT * FROM ({sql}) AS _oxy_preview LIMIT {ROW_LIMIT}");

    let result = tokio::time::timeout(
        QUERY_TIMEOUT,
        connector.execute_query(&preview_sql, ROW_LIMIT),
    )
    .await
    .map_err(|_| ToolError::Execution("query timed out after 30 seconds".into()))?
    .map_err(|e| ToolError::Execution(e.to_string()))?;

    let columns = &result.result.columns;
    let total_rows = result.result.rows.len();

    // Convert QueryResult rows to JSON array of objects.
    let rows: Vec<Value> = result
        .result
        .rows
        .iter()
        .map(|row| {
            let obj: serde_json::Map<String, Value> = columns
                .iter()
                .zip(row.0.iter())
                .map(|(col, cell)| {
                    let v = match cell {
                        agentic_core::result::CellValue::Text(s) => Value::String(s.clone()),
                        agentic_core::result::CellValue::Number(n) => {
                            serde_json::Number::from_f64(*n)
                                .map(Value::Number)
                                .unwrap_or(Value::Null)
                        }
                        agentic_core::result::CellValue::Null => Value::Null,
                    };
                    (col.clone(), v)
                })
                .collect();
            Value::Object(obj)
        })
        .collect();

    Ok(json!({
        "ok": true,
        "database": db_name,
        "columns": columns,
        "rows": rows,
        "row_count": total_rows,
    }))
}
