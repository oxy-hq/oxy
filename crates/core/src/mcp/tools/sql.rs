use crate::adapters::session_filters::SessionFilters;
use crate::config::ConfigManager;
use crate::errors::OxyError;
use rmcp::model::Tool;
use serde_json::{Map, Value};
use std::path::PathBuf;
use std::sync::Arc;

use crate::mcp::types::{EVENT_CHANNEL_SIZE, OxyTool, SQL_TOOL_PREFIX, SqlFileToolInput, ToolType};
use crate::mcp::utils::extract_sql_description;

pub fn get_sql_tool_name(sql_name: &str) -> String {
    format!("{SQL_TOOL_PREFIX}{sql_name}")
}

/// Creates an MCP tool for a SQL file
/// Generates input schema with database field
pub async fn resolve_execute_sql_tool(
    config_manager: ConfigManager,
    sql_path: PathBuf,
) -> Result<(String, OxyTool), OxyError> {
    // Convert absolute path to relative path from project root
    let config = config_manager.get_config();
    let relative_path = sql_path
        .strip_prefix(&config.project_path)
        .map_err(|_| {
            OxyError::ConfigurationError(format!(
                "SQL file path {} is not within project path {}",
                sql_path.display(),
                config.project_path.display()
            ))
        })?
        .to_str()
        .ok_or_else(|| {
            OxyError::ConfigurationError(format!(
                "Failed to convert SQL file path to string: {}",
                sql_path.display()
            ))
        })?
        .to_string();

    let content = tokio::fs::read_to_string(&sql_path).await.map_err(|e| {
        OxyError::ConfigurationError(format!(
            "Failed to read SQL file {}: {}",
            sql_path.display(),
            e
        ))
    })?;

    let file_name = sql_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| {
            OxyError::ConfigurationError(format!(
                "Failed to extract file name from {}",
                sql_path.display()
            ))
        })?
        .to_string();

    // Extract description from SQL comments
    let description = extract_sql_description(&content)
        .unwrap_or_else(|| format!("Execute {} SQL query", file_name));

    let schema = schemars::schema_for!(SqlFileToolInput);
    let schema_json = serde_json::to_value(schema)?;

    let tool_name = get_sql_tool_name(&file_name);

    let tool = Tool::new(
        tool_name.clone(),
        description,
        Arc::new(serde_json::from_value(schema_json)?),
    );

    let oxy_tool = OxyTool {
        tool,
        tool_type: ToolType::SqlFile,
        name: relative_path,
    };

    tracing::debug!(
        "Created SQL file tool '{}' from file: {}",
        tool_name,
        sql_path.display()
    );

    Ok((tool_name, oxy_tool))
}

/// Runs a SQL file tool with the given arguments
pub async fn run_sql_file_tool(
    project_manager: &crate::adapters::project::manager::ProjectManager,
    sql_file_path: String,
    arguments: Option<Map<String, Value>>,
    filters: Option<SessionFilters>,
    connections: Option<crate::config::model::ConnectionOverrides>,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    use crate::execute::Executable;
    use crate::tools::SQLExecutable;
    use rmcp::model::{CallToolResult, Content};

    let args = arguments.unwrap_or_default();
    let input: SqlFileToolInput =
        serde_json::from_value(serde_json::Value::Object(args)).map_err(|e| {
            rmcp::ErrorData::invalid_request(
                format!("Failed to parse SQL file tool input: {e}"),
                None,
            )
        })?;

    // Resolve relative path to absolute path based on project path
    let config = project_manager.config_manager.get_config();
    let absolute_sql_path = config.project_path.join(&sql_file_path);

    let sql_content = tokio::fs::read_to_string(&absolute_sql_path)
        .await
        .map_err(|e| {
            rmcp::ErrorData::internal_error(
                format!("Failed to read SQL file {}: {}", sql_file_path, e),
                None,
            )
        })?;

    let (mut execution_context, mut rx) = create_execution_context(project_manager, "mcp_sql_file");

    // Apply session filters if provided
    if let Some(session_filters) = filters {
        execution_context.filters = Some(session_filters);
    }

    // Apply connection overrides if provided
    if let Some(connection_overrides) = connections {
        execution_context.connections = Some(connection_overrides);
    }

    // Spawn a task to consume events
    tokio::spawn(async move { while rx.recv().await.is_some() {} });

    // Execute using the same executable as other tools
    let mut executable = SQLExecutable;
    let output = executable
        .execute(
            &execution_context,
            crate::tools::types::SQLInput {
                sql: sql_content,
                database: input.database.clone().unwrap_or_default(),
                dry_run_limit: None,
                name: None,
            },
        )
        .await;

    // Convert output to MCP response
    match output {
        Ok(output) => {
            let content_text = output.to_markdown();
            Ok(CallToolResult::success(vec![Content::text(content_text)]))
        }
        Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
            "Failed to execute SQL query: {e}"
        ))])),
    }
}
/// Creates an execution context for tool execution
fn create_execution_context(
    project_manager: &crate::adapters::project::manager::ProjectManager,
    kind: &str,
) -> (
    crate::execute::ExecutionContext,
    tokio::sync::mpsc::Receiver<crate::execute::types::Event>,
) {
    use crate::execute::{
        ExecutionContext,
        renderer::Renderer,
        types::{Event, Source},
    };

    let (tx, rx) = tokio::sync::mpsc::channel::<Event>(EVENT_CHANNEL_SIZE);
    let source = Source {
        parent_id: None,
        id: uuid::Uuid::new_v4().to_string(),
        kind: kind.to_string(),
    };

    let renderer = Renderer::new(minijinja::context! {});
    let execution_context = ExecutionContext::new(
        source,
        renderer,
        project_manager.clone(),
        tx,
        None,
        uuid::Uuid::nil(),
    );

    (execution_context, rx)
}
