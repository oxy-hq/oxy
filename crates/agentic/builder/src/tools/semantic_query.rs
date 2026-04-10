//! `semantic_query` tool — compile and execute a semantic layer query.
//!
//! Validates the query against the project's `.view.yml` / `.topic.yml` files,
//! compiles it to SQL via airlayer, and runs it against the configured database.
//! Useful for verifying semantic layer definitions before proposing file changes.

use std::path::Path;
use std::time::Duration;

use agentic_core::tools::{ToolDef, ToolError};
use arrow::json::ArrayWriter;
use oxy::config::model::SemanticQueryTask;
use oxy::types::SemanticQueryParams;
use serde_json::{Value, json};

const QUERY_TIMEOUT: Duration = Duration::from_secs(30);

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
    }
}

pub async fn execute_semantic_query(
    workspace_root: &Path,
    params: &Value,
    secrets_manager: Option<&oxy::adapters::secrets::SecretsManager>,
) -> Result<Value, ToolError> {
    // Build ConfigManager from project root.
    let config_manager = oxy::config::ConfigBuilder::new()
        .with_workspace_path(workspace_root)
        .map_err(|e| ToolError::Execution(format!("config error: {e}")))?
        .build()
        .await
        .map_err(|e| ToolError::Execution(format!("config error: {e}")))?;

    // Deserialize into the canonical SemanticQueryParams (same type used by the semantic query agent).
    let query: SemanticQueryParams = serde_json::from_value(params.clone())
        .map_err(|e| ToolError::BadParams(format!("invalid semantic query params: {e}")))?;

    // Cap row limit.
    let row_limit = query.limit.unwrap_or(20).min(100);

    let task = SemanticQueryTask {
        variables: query.variables.clone(),
        query,
        export: None,
    };

    // Validate against the semantic layer (loads .view.yml / .topic.yml files).
    let validated = oxy_workflow::semantic_validator_builder::validate_semantic_query_task(
        &config_manager,
        &task,
    )
    .await
    .map_err(|e| ToolError::Execution(e.to_string()))?;

    // Compile to SQL (no ExecutionContext needed).
    let sql = oxy_workflow::semantic_builder::compile_validated_to_sql(&validated, &config_manager)
        .map_err(|e| ToolError::Execution(e.to_string()))?;

    // Determine target database from view datasource annotations.
    let db_name = oxy_workflow::semantic_builder::get_database_from_validated(&validated)
        .map_err(|e| ToolError::Execution(e.to_string()))?;

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

    let connector = oxy::connector::Connector::from_database(
        &db_name,
        &config_manager,
        secrets_manager,
        Some(row_limit),
        None,
        None,
    )
    .await
    .map_err(|e| ToolError::Execution(e.to_string()))?;

    let limited_sql = format!("SELECT * FROM ({sql}) AS _oxy_semantic_preview LIMIT {row_limit}");

    let (batches, schema) = tokio::time::timeout(
        QUERY_TIMEOUT,
        connector.run_query_with_limit(&limited_sql, Some(row_limit)),
    )
    .await
    .map_err(|_| ToolError::Execution("query timed out after 30 seconds".into()))?
    .map_err(|e| ToolError::Execution(e.to_string()))?;

    let columns: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();

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
        "sql_generated": sql,
        "database": db_name,
        "columns": columns,
        "rows": rows,
        "row_count": total_rows,
    }))
}
