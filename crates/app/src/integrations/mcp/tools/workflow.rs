//! MCP workflow tool surface.
//!
//! Each `.workflow.yml` / `.procedure.yml` / `.automation.yml` file in the
//! workspace becomes an MCP tool whose name is the file stem (prefixed
//! with [`WORKFLOW_TOOL_PREFIX`]). Calling the tool runs the workflow
//! synchronously through `agentic_pipeline::workflow_run::run_inline_workflow`
//! and returns the per-step result map.
//!
//! Variable schemas are not yet exposed — every tool advertises a free-form
//! `variables` object — because the `agentic_workflow::WorkflowConfig`
//! shape doesn't yet carry an MCP-ready JSON Schema for declared variables.
//! Add this back in a follow-up if MCP clients start needing strict
//! validation.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use oxy::config::ConfigManager;
use oxy_shared::errors::OxyError;
use rmcp::model::Tool;
use serde_json::{Map, Value, json};

use crate::integrations::mcp::types::{OxyTool, ToolType, WORKFLOW_TOOL_PREFIX};

pub fn get_workflow_tool_name(workflow_name: &str) -> String {
    format!("{WORKFLOW_TOOL_PREFIX}{workflow_name}")
}

/// Strip directories + known suffixes — `path/to/foo.workflow.yml` → `foo`.
fn workflow_display_name(path: &std::path::Path) -> String {
    let stem = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workflow");
    stem.split('.').next().unwrap_or(stem).to_string()
}

/// JSON schema with a single free-form `variables` object — every workflow
/// tool advertises the same shape until per-workflow variable schemas are
/// re-introduced.
fn variables_only_schema() -> Map<String, Value> {
    let raw = json!({
        "type": "object",
        "properties": {
            "variables": {
                "type": "object",
                "description": "Optional template variables passed to the workflow."
            }
        }
    });
    serde_json::from_value(raw).expect("workflow tool schema is well-formed")
}

pub async fn get_all_workflow_tools(
    config_manager: ConfigManager,
) -> Result<HashMap<String, OxyTool>, OxyError> {
    let paths = config_manager.list_workflows().await?;
    let mut tools_map = HashMap::with_capacity(paths.len());
    for path in paths {
        let name = workflow_display_name(&path);
        let tool_name = get_workflow_tool_name(&name);
        let tool = Tool::new(tool_name.clone(), "", Arc::new(variables_only_schema()));
        tools_map.insert(
            tool_name,
            OxyTool {
                tool,
                tool_type: ToolType::Workflow,
                name,
            },
        );
    }
    Ok(tools_map)
}

pub async fn resolve_workflow_tool(
    config_manager: ConfigManager,
    workflow_path: PathBuf,
) -> Result<(String, OxyTool), OxyError> {
    let config = config_manager.get_config();
    let relative_path = workflow_path
        .strip_prefix(&config.workspace_path)
        .map_err(|_| {
            OxyError::ConfigurationError(format!(
                "Workflow path {} is not within project path {}",
                workflow_path.display(),
                config.workspace_path.display()
            ))
        })?
        .to_path_buf();
    let name = workflow_display_name(&relative_path);
    let tool_name = get_workflow_tool_name(&name);
    let tool = Tool::new(tool_name.clone(), "", Arc::new(variables_only_schema()));
    Ok((
        tool_name,
        OxyTool {
            tool,
            tool_type: ToolType::Workflow,
            name,
        },
    ))
}

pub async fn run_workflow_tool(
    workspace_manager: &oxy::adapters::workspace::manager::WorkspaceManager,
    workflow_name: String,
    arguments: Option<Map<String, Value>>,
    _filters: Option<oxy::adapters::session_filters::SessionFilters>,
    _connections: Option<oxy::config::model::ConnectionOverrides>,
    meta_variables: HashMap<String, Value>,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    use rmcp::model::{CallToolResult, Content};

    // Find a workflow file whose display name matches the requested tool.
    let paths = workspace_manager
        .config_manager
        .list_workflows()
        .await
        .map_err(|e| {
            rmcp::ErrorData::internal_error(format!("Failed to list workflows: {e}"), None)
        })?;
    let workflow_path = paths
        .into_iter()
        .find(|p| workflow_display_name(p) == workflow_name)
        .ok_or_else(|| {
            rmcp::ErrorData::invalid_request(format!("Workflow '{workflow_name}' not found"), None)
        })?;

    let yaml = tokio::fs::read_to_string(&workflow_path)
        .await
        .map_err(|e| {
            rmcp::ErrorData::internal_error(format!("read {}: {e}", workflow_path.display()), None)
        })?;
    let workflow: agentic_workflow::WorkflowConfig = serde_yaml::from_str(&yaml)
        .map_err(|e| rmcp::ErrorData::internal_error(format!("parse workflow: {e}"), None))?;

    // Merge arg-level + meta-level variables. Args win — they were given
    // explicitly by the LLM in this tool call.
    let mut merged: serde_json::Map<String, Value> = meta_variables.into_iter().collect();
    if let Some(args) = arguments
        && let Some(arg_vars) = args.get("variables").and_then(|v| v.as_object())
    {
        for (k, v) in arg_vars {
            merged.insert(k.clone(), v.clone());
        }
    }
    let variables = if merged.is_empty() {
        None
    } else {
        Some(Value::Object(merged))
    };

    let project_ctx = std::sync::Arc::new(crate::agentic_wiring::OxyProjectContext::new(
        workspace_manager.clone(),
    ));
    let workspace: std::sync::Arc<dyn agentic_workflow::WorkspaceContext> = project_ctx;
    let results = agentic_pipeline::workflow_run::run_inline_workflow_with(
        workspace.as_ref(),
        workflow,
        variables,
        None,
    )
    .await
    .map_err(|e| rmcp::ErrorData::internal_error(format!("inline workflow: {e}"), None))?;

    let body = serde_json::to_string_pretty(&results).unwrap_or_else(|_| "{}".to_string());
    Ok(CallToolResult::success(vec![Content::text(body)]))
}
