//! `semantic_query` tool — compile and execute a semantic layer query.
//!
//! Validates the query against the project's `.view.yml` / `.topic.yml` files,
//! compiles it to SQL via the semantic compiler, and runs it against the
//! configured database. Useful for verifying semantic layer definitions before
//! proposing file changes.

use std::time::Duration;

use agentic_core::result::{CellValue, QueryResult};
use agentic_core::tools::{ToolDef, ToolError};
use serde_json::{Value, json};

use crate::database::BuilderDatabaseProvider;
use crate::semantic::{BuilderSemanticCompiler, SemanticCompilationResult};

const QUERY_TIMEOUT: Duration = Duration::from_secs(30);
/// The rollup tier's slice of [`QUERY_TIMEOUT`]. A pre-aggregation read is
/// supposed to be milliseconds; capping it well under the whole budget is what
/// leaves the warehouse fallback time to answer when the rollup hangs.
const PREAGG_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_ROW_LIMIT: u64 = 100;

pub fn semantic_query_def() -> ToolDef {
    // Hand-written schema compatible with OpenAI strict mode (no oneOf/anyOf).
    // The schemars-generated schema uses oneOf for Rust enums which strict mode rejects.
    ToolDef {
        name: "semantic_query",
        description: "Run a semantic layer query against the project's configured database. \
                      Validates the query against the semantic model (view/topic files), \
                      compiles it to SQL, executes it, and returns the results. \
                      Useful for verifying that semantic definitions (.view.yml, .topic.yml) \
                      are correct before proposing file changes. \
                      Specify 'topic' plus at least one of 'dimensions' or 'measures'. \
                      Returns {ok, sql_generated, database, columns, rows, row_count} on success \
                      or {ok: false, error} on failure.",
        parameters: json!({
            "type": "object",
            "properties": {
                "topic": {
                    "type": ["string", "null"],
                    "description": "Topic name to query against. Null if querying views directly."
                },
                "measures": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of measures. Format: <view_name>.<measure_name>"
                },
                "dimensions": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of dimensions. Format: <view_name>.<dimension_name>"
                },
                "time_dimensions": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "dimension": {
                                "type": "string",
                                "description": "Time dimension name. Format: <view_name>.<dimension_name>"
                            },
                            "granularity": {
                                "type": ["string", "null"],
                                "description": "Granularity: year, quarter, month, week, day, hour, minute, or second. Null for no grouping."
                            }
                        },
                        "required": ["dimension", "granularity"],
                        "additionalProperties": false
                    },
                    "description": "Time dimensions with optional granularity."
                },
                "filters": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "field": {
                                "type": "string",
                                "description": "Field to filter on. Format: <view_name>.<field_name>"
                            },
                            "op": {
                                "type": "string",
                                "description": "Filter operator: eq, neq, gt, gte, lt, lte, in, not_in, in_date_range, not_in_date_range"
                            },
                            "value": {
                                "type": ["string", "null"],
                                "description": "Scalar value for eq/neq/gt/gte/lt/lte operators. Null for array operators."
                            },
                            "values": {
                                "type": ["array", "null"],
                                "items": { "type": "string" },
                                "description": "Array of values for in/not_in operators. Null for scalar operators."
                            }
                        },
                        "required": ["field", "op", "value", "values"],
                        "additionalProperties": false
                    },
                    "description": "Filters to apply."
                },
                "orders": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "field": { "type": "string", "description": "Field to order by." },
                            "direction": { "type": "string", "description": "asc or desc." }
                        },
                        "required": ["field", "direction"],
                        "additionalProperties": false
                    },
                    "description": "Order-by clauses."
                },
                "limit": {
                    "type": ["integer", "null"],
                    "description": "Max rows to return (capped at 100). Null defaults to 20."
                },
                "offset": {
                    "type": ["integer", "null"],
                    "description": "Number of rows to skip. Null defaults to 0."
                }
            },
            "required": ["topic", "measures", "dimensions", "time_dimensions", "filters", "orders", "limit", "offset"],
            "additionalProperties": false
        }),
        ..Default::default()
    }
}

pub async fn execute_semantic_query(
    params: &Value,
    db_provider: &dyn BuilderDatabaseProvider,
    semantic_compiler: &dyn BuilderSemanticCompiler,
) -> Result<Value, ToolError> {
    let compiled = semantic_compiler.compile(params).await?;

    let row_limit = params["limit"].as_u64().unwrap_or(20).min(MAX_ROW_LIMIT);

    let (sql_for_display, database_label, used_preaggregation, query_result) = match compiled {
        SemanticCompilationResult::Warehouse { sql, database_name } => {
            let limited_sql =
                format!("SELECT * FROM ({sql}) AS _oxy_semantic_preview LIMIT {row_limit}");
            let connector = db_provider.get_connector(&database_name).await?;
            let execution = tokio::time::timeout(
                QUERY_TIMEOUT,
                connector.execute_query(&limited_sql, row_limit),
            )
            .await
            .map_err(|_| ToolError::Execution("query timed out after 30 seconds".into()))?
            .map_err(|e| ToolError::Execution(e.to_string()))?;
            (sql, database_name, false, execution.result)
        }
        SemanticCompilationResult::Preaggregation {
            preagg_sql,
            handle,
            warehouse_sql,
            warehouse_database,
        } => {
            // One deadline covers both tiers, so a rollup that hangs can't turn
            // a 30s budget into 60s behind a message that says 30. The rollup
            // is the *fast* path, so it only gets a slice of it — leaving the
            // warehouse enough time to actually answer when the rollup hangs,
            // which is exactly the case the fallback exists for.
            //
            // Note what this timeout does and doesn't do: `execute_preagg`
            // runs the read in `spawn_blocking`, so dropping the future here
            // detaches that task rather than cancelling it — the thread and
            // its DuckDB connection live on beside the fallback. What actually
            // bounds the read is `SET http_timeout` / `SET http_retries` in
            // `oxy_shared::duckdb_s3::s3_setup_sql`, which every caller gets,
            // including the analytics path that has no timeout at all. This
            // one only bounds how long *we* wait.
            let deadline = tokio::time::Instant::now() + QUERY_TIMEOUT;
            let rollup = tokio::time::timeout(
                PREAGG_ATTEMPT_TIMEOUT,
                semantic_compiler.execute_preagg(&preagg_sql, &handle, row_limit),
            )
            .await
            // A rollup that times out has not answered, same as one that
            // errored — fold both into the `match` so a hung `httpfs` read
            // against an unreachable endpoint falls back instead of failing.
            .unwrap_or_else(|_| {
                Err(ToolError::Execution(format!(
                    "rollup read timed out after {}s",
                    PREAGG_ATTEMPT_TIMEOUT.as_secs()
                )))
            });

            match rollup {
                Ok(result) => (warehouse_sql, warehouse_database, true, result),
                // A rollup that won't read is not a failed query — the same
                // question has a warehouse answer, and the compilation result
                // carries the SQL for it. Report `is_preagg: false` so the
                // model is told which path actually answered.
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "semantic_query: rollup read failed; answering from the warehouse"
                    );
                    let limited_sql = format!(
                        "SELECT * FROM ({warehouse_sql}) AS _oxy_semantic_preview LIMIT {row_limit}"
                    );
                    let connector = db_provider.get_connector(&warehouse_database).await?;
                    let execution = tokio::time::timeout_at(
                        deadline,
                        connector.execute_query(&limited_sql, row_limit),
                    )
                    .await
                    .map_err(|_| {
                        ToolError::Execution(format!(
                            "query timed out after {}s",
                            QUERY_TIMEOUT.as_secs()
                        ))
                    })?
                    .map_err(|e| ToolError::Execution(e.to_string()))?;
                    (warehouse_sql, warehouse_database, false, execution.result)
                }
            }
        }
    };

    let rows = rows_to_json(&query_result);

    Ok(json!({
        "ok": true,
        "sql_generated": sql_for_display,
        "database": database_label,
        "is_preagg": used_preaggregation,
        "columns": query_result.columns,
        "rows": rows,
        "row_count": query_result.rows.len(),
    }))
}

fn rows_to_json(result: &QueryResult) -> Vec<Value> {
    result
        .rows
        .iter()
        .map(|row| {
            let obj: serde_json::Map<String, Value> = result
                .columns
                .iter()
                .zip(row.0.iter())
                .map(|(col, cell)| {
                    let v = match cell {
                        CellValue::Text(s) => Value::String(s.clone()),
                        CellValue::Number(n) => serde_json::Number::from_f64(*n)
                            .map(Value::Number)
                            .unwrap_or(Value::Null),
                        CellValue::Null => Value::Null,
                    };
                    (col.clone(), v)
                })
                .collect();
            Value::Object(obj)
        })
        .collect()
}
