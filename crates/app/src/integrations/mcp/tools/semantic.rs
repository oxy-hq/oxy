use crate::integrations::mcp::types::{
    EVENT_CHANNEL_SIZE, OxyTool, SEMANTIC_TOOL_PREFIX, SemanticTopicToolInput, ToolType,
};
use oxy::adapters::semantic_tool_description::build_semantic_topic_description;
use oxy::adapters::session_filters::SessionFilters;
use oxy::config::ConfigManager;
use oxy::config::WorkingCopy;
use oxy_semantic::parse_semantic_layer_from_dir;
use oxy_shared::errors::OxyError;
use rmcp::model::Tool;
use serde_json::{Map, Value};
use std::path::PathBuf;
use std::sync::Arc;

pub fn get_semantic_tool_name(topic_name: &str) -> String {
    format!("{SEMANTIC_TOOL_PREFIX}{topic_name}")
}

/// Creates an MCP tool for a semantic topic file
/// Generates input schema with dimensions, metrics, filters, limit, and order_by
pub async fn resolve_semantic_tool(
    config_manager: ConfigManager<WorkingCopy>,
    topic_path: PathBuf,
) -> Result<(String, OxyTool), OxyError> {
    use oxy_semantic::models::Topic;

    // Load the semantic model to get view metadata
    let semantic_layer =
        parse_semantic_layer_from_dir(config_manager.semantics_scan_path())?.semantic_layer;

    let content = tokio::fs::read_to_string(&topic_path).await.map_err(|e| {
        OxyError::ConfigurationError(format!(
            "Failed to read topic file {}: {}",
            topic_path.display(),
            e
        ))
    })?;

    let topic: Topic = serde_yaml::from_str(&content).map_err(|e| {
        OxyError::ConfigurationError(format!(
            "Failed to parse topic file {}: {}",
            topic_path.display(),
            e
        ))
    })?;

    let topic_name = topic.name.clone();

    let schema = schemars::schema_for!(SemanticTopicToolInput);
    let schema_json = serde_json::to_value(schema)?;

    let tool_name = get_semantic_tool_name(&topic_name);

    // Build detailed description with semantic model metadata
    let description = build_semantic_topic_description(&topic, &semantic_layer);

    let tool = Tool::new(
        tool_name.clone(),
        description,
        Arc::new(serde_json::from_value(schema_json)?),
    );

    let oxy_tool = OxyTool {
        tool,
        tool_type: ToolType::SemanticTopic,
        name: topic_name.clone(),
    };

    tracing::debug!(
        "Created semantic topic tool '{}' from file: {}",
        tool_name,
        topic_path.display()
    );

    Ok((tool_name, oxy_tool))
}

/// Runs a semantic topic tool with the given arguments.
///
/// Builds an in-memory single-step `AutomationConfig` whose only task is a
/// `semantic_query`, then drives it through the inline automation runner.
/// This re-uses the same compilation + execution pipeline that the new
/// `/agentic-workflows` HTTP surface uses, instead of duplicating the
/// semantic plumbing into the MCP layer.
pub async fn run_semantic_topic_tool(
    workspace_manager: &oxy::adapters::workspace::manager::WorkspaceManager<WorkingCopy>,
    topic_name: String,
    arguments: Option<Map<String, Value>>,
    _filters: Option<SessionFilters>,
    _connections: Option<oxy::config::model::ConnectionOverrides>,
    _meta_variables: std::collections::HashMap<String, Value>,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    use rmcp::model::{CallToolResult, Content};

    let args = arguments.unwrap_or_default();
    let input: SemanticTopicToolInput =
        match serde_json::from_value(serde_json::Value::Object(args)) {
            Ok(input) => input,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to parse semantic topic tool input: {e}"
                ))]));
            }
        };

    // Project the SemanticTopicToolInput onto the on-the-wire shape that
    // `agentic_automation::SemanticQueryConfig` expects via JSON parsing.
    // Fields default-empty so omitting them in the MCP call works.
    let mut step_config = serde_json::Map::new();
    step_config.insert("topic".to_string(), Value::String(topic_name));
    if let Some(measures) = input.measures {
        step_config.insert("measures".into(), Value::Array(string_values(measures)));
    }
    if let Some(dimensions) = input.dimensions {
        step_config.insert("dimensions".into(), Value::Array(string_values(dimensions)));
    }
    if let Some(orders) = input.order_by {
        step_config.insert("orders".into(), Value::Array(serde_json_array(orders)));
    }
    if let Some(filters) = input.filters {
        step_config.insert("filters".into(), Value::Array(serde_json_array(filters)));
    }
    if let Some(time_dims) = input.time_dimensions {
        step_config.insert(
            "time_dimensions".into(),
            Value::Array(serde_json_array(time_dims)),
        );
    }
    if let Some(limit) = input.limit {
        step_config.insert("limit".into(), Value::Number(limit.into()));
    }

    // Build the task body imperatively — `serde_json::json!` doesn't support
    // splatting an existing map, but the `semantic_query` variant is opaque
    // on the automation side so all the SemanticQueryConfig fields live at
    // this object's root alongside `name` and `type`.
    let mut task_body = step_config;
    task_body.insert("name".into(), Value::String("semantic".into()));
    task_body.insert("type".into(), Value::String("semantic_query".into()));
    let automation_value = serde_json::json!({
        "name": "mcp-semantic-topic",
        "tasks": [Value::Object(task_body)],
    });
    let automation_config: agentic_automation::AutomationConfig =
        match serde_json::from_value(automation_value) {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to build inline automation for semantic topic: {e}"
                ))]));
            }
        };

    let project_ctx = std::sync::Arc::new(crate::agentic_wiring::OxyProjectContext::new(
        workspace_manager.clone(),
    ));
    let workspace: std::sync::Arc<dyn agentic_automation::WorkspaceContext> = project_ctx;
    let results = match agentic_pipeline::automation_run::run_inline_automation_with(
        workspace.as_ref(),
        automation_config,
        None,
        None,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to execute semantic query: {e}"
            ))]));
        }
    };

    let body = match results.get("semantic") {
        Some(value) => serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
        None => "{}".to_string(),
    };
    Ok(CallToolResult::success(vec![Content::text(body)]))
}

fn string_values(items: Vec<String>) -> Vec<Value> {
    items.into_iter().map(Value::String).collect()
}

fn serde_json_array<T: serde::Serialize>(items: Vec<T>) -> Vec<Value> {
    items
        .into_iter()
        .filter_map(|item| serde_json::to_value(item).ok())
        .collect()
}

fn create_execution_context(
    workspace_manager: &oxy::adapters::workspace::manager::WorkspaceManager<WorkingCopy>,
    kind: &str,
) -> (
    oxy::exec_runtime::ExecutionContext,
    tokio::sync::mpsc::Receiver<oxy::exec_types::Event>,
) {
    use oxy::exec_runtime::{ExecutionContext, renderer::Renderer};
    use oxy::exec_types::{Event, Source};

    let (tx, rx) = tokio::sync::mpsc::channel::<Event>(EVENT_CHANNEL_SIZE);
    let source = Source {
        parent_id: None,
        id: uuid::Uuid::new_v4().to_string(),
        kind: kind.to_string(),
    };

    let renderer = Renderer::new(minijinja::context! {});
    let execution_context =
        ExecutionContext::new(source, renderer, workspace_manager.clone(), tx, None);

    (execution_context, rx)
}
