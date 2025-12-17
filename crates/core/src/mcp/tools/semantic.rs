use crate::adapters::semantic_tool_description::build_semantic_topic_description;
use crate::adapters::session_filters::SessionFilters;
use crate::config::ConfigManager;
use crate::errors::OxyError;
use crate::mcp::types::{
    EVENT_CHANNEL_SIZE, OxyTool, SEMANTIC_TOOL_PREFIX, SemanticTopicToolInput, ToolType,
};
use oxy_semantic::parse_semantic_layer_from_dir;
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
    config_manager: ConfigManager,
    topic_path: PathBuf,
) -> Result<(String, OxyTool), OxyError> {
    use oxy_semantic::models::Topic;

    // Load the semantic layer to get view metadata
    let semantic_layer = parse_semantic_layer_from_dir(
        config_manager.semantics_path(),
        config_manager.get_globals_registry(),
    )?
    .semantic_layer;

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

    // Build detailed description with semantic layer metadata
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

/// Runs a semantic topic tool with the given arguments
pub async fn run_semantic_topic_tool(
    project_manager: &crate::adapters::project::manager::ProjectManager,
    topic_name: String,
    arguments: Option<Map<String, Value>>,
    filters: Option<SessionFilters>,
    connections: Option<crate::config::model::ConnectionOverrides>,
    meta_variables: std::collections::HashMap<String, Value>,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    use crate::config::model::SemanticQueryTask;
    use crate::execute::Executable;
    use crate::service::types::SemanticQueryParams;
    use crate::workflow::SemanticQueryExecutable;
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

    // Extract variables from arguments
    let arg_variables = input
        .variables
        .clone()
        .map(|v| v.into_iter().collect::<std::collections::HashMap<_, _>>())
        .unwrap_or_default();

    // No defaults for semantic topics (they don't have a variable schema)
    let default_variables = std::collections::HashMap::new();

    // Merge variables using proper precedence: defaults < arguments < meta
    let merged_variables =
        crate::mcp::variables::merge_variables(default_variables, meta_variables, arg_variables);

    let query_params = SemanticQueryParams {
        topic: Some(topic_name.clone()),
        measures: input.measures.unwrap_or_default(),
        dimensions: input.dimensions.unwrap_or_default(),
        filters: input.filters.unwrap_or_default(),
        orders: input.order_by.unwrap_or_default(),
        limit: input.limit,
        offset: None,
        variables: if merged_variables.is_empty() {
            None
        } else {
            Some(merged_variables.clone())
        },
    };

    let task = SemanticQueryTask {
        query: query_params,
        export: None,
        variables: if merged_variables.is_empty() {
            None
        } else {
            Some(merged_variables)
        },
    };

    let (mut execution_context, mut rx) =
        create_execution_context(project_manager, "mcp_semantic_query");

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

    // Validate and execute the semantic query
    let validated_query =
        match crate::workflow::validate_semantic_query_task(&project_manager.config_manager, &task)
            .await
        {
            Ok(query) => query,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to validate semantic query: {e}"
                ))]));
            }
        };

    let mut executable = SemanticQueryExecutable::new();
    let output = executable
        .execute(&execution_context, validated_query)
        .await;

    // Convert output to MCP response
    match output {
        Ok(output) => {
            let content_text = output.to_markdown();
            Ok(CallToolResult::success(vec![Content::text(content_text)]))
        }
        Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
            "Failed to execute semantic query: {e}"
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
    let execution_context =
        ExecutionContext::new(source, renderer, project_manager.clone(), tx, None);

    (execution_context, rx)
}
