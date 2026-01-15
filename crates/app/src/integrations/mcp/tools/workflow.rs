use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::model::Tool;
use serde_json::{Map, Value, json};

use crate::server::service::workflow::{get_workflow, list_workflows, run_workflow};
use oxy::adapters::session_filters::SessionFilters;
use oxy::checkpoint::types::RetryStrategy;
use oxy::config::ConfigManager;
use oxy_shared::errors::OxyError;
use oxy_workflow::loggers::NoopLogger;

use crate::integrations::mcp::types::{OxyTool, ToolType, WORKFLOW_TOOL_PREFIX};
use crate::integrations::mcp::utils::json_to_hashmap;

pub fn get_workflow_tool_name(workflow_name: &str) -> String {
    format!("{WORKFLOW_TOOL_PREFIX}{workflow_name}")
}

/// Gets all workflow tools from the project
pub async fn get_all_workflow_tools(
    config_manager: ConfigManager,
) -> Result<HashMap<String, OxyTool>, OxyError> {
    let mut tools_map = HashMap::new();
    let workflows = list_workflows(config_manager.clone()).await?;

    for workflow in workflows {
        let workflow_config =
            get_workflow(PathBuf::from(workflow.path.clone()), config_manager.clone()).await?;

        let tool_name = get_workflow_tool_name(workflow.name.as_str());
        let mut schema_json;
        if let Some(vars) = &workflow_config.variables {
            let schema = serde_json::to_value(vars.variables.clone())?;
            schema_json = serde_json::from_value::<Map<String, Value>>(json!({
                "type": "object",
                "properties": schema
            }))?;
        } else {
            schema_json = Map::new();
            schema_json.insert("type".to_string(), Value::String("object".to_string()));
        }
        let tool = Tool::new(
            tool_name.clone(),
            workflow_config.description,
            Arc::new(schema_json),
        );

        let oxy_tool = OxyTool {
            tool: tool.clone(),
            tool_type: ToolType::Workflow,
            name: workflow.name,
        };
        tools_map.insert(tool_name, oxy_tool);
    }
    Ok(tools_map)
}

/// Gets a workflow tool from a specific file path
pub async fn resolve_workflow_tool(
    config_manager: ConfigManager,
    workflow_path: PathBuf,
) -> Result<(String, OxyTool), OxyError> {
    // Convert absolute path to relative path from project root
    let config = config_manager.get_config();
    let relative_path = workflow_path
        .strip_prefix(&config.project_path)
        .map_err(|_| {
            OxyError::ConfigurationError(format!(
                "Workflow path {} is not within project path {}",
                workflow_path.display(),
                config.project_path.display()
            ))
        })?
        .to_path_buf();

    let workflow_config = get_workflow(relative_path.clone(), config_manager.clone()).await?;

    let tool_name = get_workflow_tool_name(workflow_config.name.as_str());
    let mut schema_json;
    if let Some(vars) = &workflow_config.variables {
        let schema = serde_json::to_value(vars.variables.clone())?;
        schema_json = serde_json::from_value::<Map<String, Value>>(json!({
            "type": "object",
            "properties": schema
        }))?;
    } else {
        schema_json = Map::new();
        schema_json.insert("type".to_string(), Value::String("object".to_string()));
    }

    let tool = Tool::new(
        tool_name.clone(),
        workflow_config.description,
        Arc::new(schema_json),
    );

    let oxy_tool = OxyTool {
        tool: tool.clone(),
        tool_type: ToolType::Workflow,
        name: workflow_config.name.clone(),
    };

    tracing::debug!(
        "Created workflow tool '{}' from file: {}",
        tool_name,
        workflow_path.display()
    );

    Ok((tool_name, oxy_tool))
}

/// Runs a workflow tool with the given arguments
pub async fn run_workflow_tool(
    project_manager: &oxy::adapters::project::manager::ProjectManager,
    workflow_name: String,
    arguments: Option<Map<String, Value>>,
    filters: Option<SessionFilters>,
    connections: Option<oxy::config::model::ConnectionOverrides>,
    meta_variables: std::collections::HashMap<String, Value>,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    use rmcp::model::CallToolResult;

    // Extract variables from arguments
    let arg_variables = arguments
        .as_ref()
        .and_then(|args| args.get("variables"))
        .and_then(|v| v.as_object())
        .map(|v| json_to_hashmap(v.to_owned()))
        .unwrap_or_default();

    // Get workflow info to extract default variables (if any)
    let workflows = list_workflows(project_manager.config_manager.clone())
        .await
        .map_err(|e| {
            rmcp::ErrorData::internal_error(format!("Failed to list workflows: {e}"), None)
        })?;

    let workflow_info = workflows
        .iter()
        .find(|w| w.name == workflow_name)
        .ok_or_else(|| {
            rmcp::ErrorData::invalid_request(format!("Workflow {workflow_name} not found"), None)
        })?;

    // For now, we'll use an empty HashMap as defaults
    let default_variables = HashMap::new();

    // Merge variables using proper precedence: defaults < arguments < meta
    let merged_variables = crate::integrations::mcp::variables::merge_variables(
        default_variables,
        meta_variables,
        arg_variables,
    );

    let output = run_workflow(
        &PathBuf::from(workflow_info.path.clone()),
        NoopLogger {},
        RetryStrategy::NoRetry {
            variables: Some(merged_variables.into_iter().collect()),
        },
        project_manager.clone(),
        filters,
        connections,
        None, // No globals override from MCP
        Some(crate::service::agent::ExecutionSource::Mcp {
            session_id: None, // MCP doesn't have session tracking yet
        }),
        None, // No authenticated user in MCP context
    )
    .await
    .map_err(|e| rmcp::ErrorData::internal_error(format!("Failed to run workflow: {e}"), None))?;

    Ok(CallToolResult::success(vec![output.try_into().map_err(
        |_err| {
            rmcp::ErrorData::internal_error(
                "Failed to convert from workflow output into mcp output".to_string(),
                None,
            )
        },
    )?]))
}
