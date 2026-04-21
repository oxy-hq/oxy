//! Executes individual workflow steps.
//!
//! Dispatches by task type: SQL queries run via `agentic-connector`,
//! semantic queries compile via airlayer then execute, and unsupported
//! types return clear error messages.

use agentic_core::result::CellValue;
use serde_json::{Value, json};

use crate::config::{SemanticQueryConfig, TaskType};
use crate::workspace::WorkspaceContext;

/// Default row limit for step execution results.
const DEFAULT_SAMPLE_LIMIT: u64 = 10_000;

/// Execute a single workflow step and return its output as JSON.
pub async fn run_workflow_step(
    workspace: &dyn WorkspaceContext,
    step_config: Value,
    _render_context: Value,
    _workflow_context: Value,
) -> Result<String, String> {
    let task: crate::config::TaskConfig = serde_json::from_value(step_config)
        .map_err(|e| format!("failed to deserialize step config: {e}"))?;

    let result = match &task.task_type {
        TaskType::ExecuteSql(cfg) => execute_sql(workspace, cfg).await,
        TaskType::SemanticQuery(cfg) => execute_semantic_query(workspace, cfg).await,

        // These should never reach step_executor — handled by orchestrator.
        TaskType::Agent(_) => Err("agent tasks are delegated via TaskSpec::Agent".into()),
        TaskType::Formatter(_) => Err("formatter tasks execute inline in orchestrator".into()),
        TaskType::Conditional(_) => Err("conditional tasks execute inline in orchestrator".into()),
        TaskType::LoopSequential(_) => Err("loops are handled as fan-out by orchestrator".into()),
        TaskType::SubWorkflow(_) => {
            Err("sub-workflows are delegated via TaskSpec::Workflow".into())
        }

        TaskType::OmniQuery(cfg) => execute_omni_query(workspace, cfg).await,
        TaskType::LookerQuery(cfg) => execute_looker_query(workspace, cfg).await,

        // Not yet supported in the agentic pipeline.
        TaskType::Visualize(_) => Err("visualize not yet supported".into()),
        TaskType::Unknown => Err("unknown task type".into()),
    }?;

    serde_json::to_string(&result).map_err(|e| format!("failed to serialize result: {e}"))
}

/// Execute a raw SQL query via the database connector.
async fn execute_sql(workspace: &dyn WorkspaceContext, cfg: &Value) -> Result<Value, String> {
    let database = cfg
        .get("database")
        .and_then(|v| v.as_str())
        .ok_or("execute_sql: missing 'database' field")?;

    // Support both inline sql and sql_file.
    let sql = if let Some(q) = cfg.get("sql_query").and_then(|v| v.as_str()) {
        q.to_string()
    } else if let Some(path) = cfg.get("sql_file").and_then(|v| v.as_str()) {
        let full_path = workspace.workspace_path().join(path);
        std::fs::read_to_string(&full_path)
            .map_err(|e| format!("failed to read SQL file {}: {e}", full_path.display()))?
    } else {
        return Err("execute_sql: need 'sql_query' or 'sql_file'".into());
    };

    let connector = workspace.get_connector(database).await?;
    let exec_result = connector
        .execute_query(&sql, DEFAULT_SAMPLE_LIMIT)
        .await
        .map_err(|e| format!("SQL execution failed: {e}"))?;

    Ok(query_result_to_json(&exec_result.result))
}

/// Compile a semantic query via airlayer and execute the resulting SQL.
async fn execute_semantic_query(
    workspace: &dyn WorkspaceContext,
    cfg: &Value,
) -> Result<Value, String> {
    let query_config: SemanticQueryConfig = serde_json::from_value(cfg.clone())
        .map_err(|e| format!("failed to parse semantic query config: {e}"))?;

    let scan_path = workspace.workspace_path();
    let databases = workspace.database_configs();

    let (sql, database_name) =
        crate::semantic::resolve_and_compile(scan_path, &databases, &query_config)
            .map_err(|e| format!("semantic compilation failed: {e}"))?;

    let connector = workspace.get_connector(&database_name).await?;
    let exec_result = connector
        .execute_query(&sql, DEFAULT_SAMPLE_LIMIT)
        .await
        .map_err(|e| format!("semantic query execution failed: {e}"))?;

    Ok(query_result_to_json(&exec_result.result))
}

/// Convert a `QueryResult` to the JSON format expected by downstream steps.
fn query_result_to_json(result: &agentic_core::result::QueryResult) -> Value {
    let rows: Vec<Value> = result
        .rows
        .iter()
        .map(|row| {
            let cells: Vec<Value> = row
                .0
                .iter()
                .map(|cell| match cell {
                    CellValue::Text(s) => Value::String(s.clone()),
                    CellValue::Number(n) => serde_json::Number::from_f64(*n)
                        .map(Value::Number)
                        .unwrap_or(Value::Null),
                    CellValue::Null => Value::Null,
                })
                .collect();
            Value::Array(cells)
        })
        .collect();

    json!({
        "columns": result.columns,
        "rows": rows,
        "row_count": result.total_row_count,
        "truncated": result.truncated,
    })
}

/// Execute an Omni query via the standalone API client.
async fn execute_omni_query(
    workspace: &dyn WorkspaceContext,
    cfg: &Value,
) -> Result<Value, String> {
    let integration = cfg
        .get("integration")
        .and_then(|v| v.as_str())
        .ok_or("omni_query: missing 'integration' field")?;
    let topic = cfg
        .get("topic")
        .and_then(|v| v.as_str())
        .ok_or("omni_query: missing 'topic' field")?;

    let fields: Vec<String> = cfg
        .get("fields")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    if fields.is_empty() {
        return Err("omni_query: at least one field is required".into());
    }

    let limit = cfg.get("limit").and_then(|v| v.as_u64()).map(|l| l as u32);

    // Get integration credentials.
    let config = workspace.get_integration(integration).await?;
    let crate::workspace::IntegrationConfig::Omni { base_url, api_key } = config else {
        return Err(format!(
            "integration '{integration}' is not an Omni integration"
        ));
    };

    let client = omni::OmniApiClient::new(base_url, api_key)
        .map_err(|e| format!("omni client error: {e}"))?;

    let mut query_builder = omni::QueryStructure::builder()
        .topic(topic)
        .fields(fields.clone());
    if let Some(l) = limit {
        query_builder = query_builder.limit(l);
    }
    let query_structure = query_builder
        .build()
        .map_err(|e| format!("omni query build error: {e}"))?;

    let request = omni::QueryRequest::builder()
        .query(query_structure)
        .build()
        .map_err(|e| format!("omni request build error: {e}"))?;

    let response = client
        .execute_query(request)
        .await
        .map_err(|e| format!("omni query failed: {e}"))?;

    // Omni returns base64-encoded Arrow IPC in `result`. Extract available
    // metadata from the summary and return the generated SQL if available,
    // letting downstream consumers know what was queried.
    let sql = response
        .summary
        .as_ref()
        .and_then(|s| s.display_sql.clone())
        .unwrap_or_default();

    let field_names: Vec<String> = response
        .summary
        .as_ref()
        .and_then(|s| s.fields.as_ref())
        .map(|f| f.keys().cloned().collect())
        .unwrap_or_else(|| fields.clone());

    // Return metadata — full Arrow decoding would require the `arrow` crate.
    // The SQL can be executed via a database connector for full data access.
    Ok(json!({
        "columns": field_names,
        "rows": [],
        "row_count": 0,
        "sql": sql,
        "text": format!("Omni query executed against topic '{}'. SQL: {}", topic, sql),
    }))
}

/// Execute a Looker query via the standalone API client.
async fn execute_looker_query(
    workspace: &dyn WorkspaceContext,
    cfg: &Value,
) -> Result<Value, String> {
    let integration = cfg
        .get("integration")
        .and_then(|v| v.as_str())
        .ok_or("looker_query: missing 'integration' field")?;
    let model = cfg
        .get("model")
        .and_then(|v| v.as_str())
        .ok_or("looker_query: missing 'model' field")?;
    let explore = cfg
        .get("explore")
        .and_then(|v| v.as_str())
        .ok_or("looker_query: missing 'explore' field")?;

    let fields: Vec<String> = cfg
        .get("fields")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    if fields.is_empty() {
        return Err("looker_query: at least one field is required".into());
    }

    let limit = cfg.get("limit").and_then(|v| v.as_i64());

    // Get integration credentials.
    let config = workspace.get_integration(integration).await?;
    let crate::workspace::IntegrationConfig::Looker {
        base_url,
        client_id,
        client_secret,
    } = config
    else {
        return Err(format!(
            "integration '{integration}' is not a Looker integration"
        ));
    };

    let auth_config = oxy_looker::LookerAuthConfig {
        base_url,
        client_id,
        client_secret,
    };
    let mut client = oxy_looker::LookerApiClient::new(auth_config)
        .map_err(|e| format!("looker client error: {e}"))?;

    let request = oxy_looker::InlineQueryRequest {
        model: model.to_string(),
        view: explore.to_string(),
        fields: fields.clone(),
        limit: limit.or(Some(10_000)),
        filters: None,
        filter_expression: None,
        sorts: None,
        query_timezone: None,
        pivots: None,
        fill_fields: None,
    };

    let response = client
        .run_inline_query(request)
        .await
        .map_err(|e| format!("looker query failed: {e}"))?;

    // Convert response data (Vec<HashMap<String, Value>>) to columns + rows.
    let columns: Vec<String> = fields;
    let rows: Vec<Value> = response
        .data
        .iter()
        .map(|row| {
            let cells: Vec<Value> = columns
                .iter()
                .map(|col| row.get(col).cloned().unwrap_or(Value::Null))
                .collect();
            Value::Array(cells)
        })
        .collect();

    let row_count = rows.len() as u64;
    Ok(json!({
        "columns": columns,
        "rows": rows,
        "row_count": row_count,
        "truncated": false,
    }))
}

/// Extract step results from a JSON output into a normalized format.
pub fn extract_workflow_steps(output: &Value) -> Vec<Value> {
    match output {
        Value::Array(arr) => arr.clone(),
        Value::Object(map) => map
            .iter()
            .filter(|(_, v)| !v.is_null())
            .take(20)
            .map(|(name, value)| {
                json!({
                    "step_name": name,
                    "text": value.to_string(),
                })
            })
            .collect(),
        other => vec![json!({
            "step_name": "result",
            "text": other.to_string(),
        })],
    }
}
