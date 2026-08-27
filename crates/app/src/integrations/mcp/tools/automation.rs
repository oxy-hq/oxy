//! MCP automation tool surface.
//!
//! Each `.automation.yml` / `.procedure.yml` file in the
//! workspace becomes an MCP tool whose name is the file stem (prefixed
//! with [`WORKFLOW_TOOL_PREFIX`]). Calling the tool runs the automation
//! synchronously through `agentic_pipeline::automation_run::run_inline_automation`
//! and returns the per-step result map.
//!
//! Variable schemas are not yet exposed — every tool advertises a free-form
//! `variables` object — because the `agentic_automation::AutomationConfig`
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
use oxy::config::WorkingCopy;

pub fn get_automation_tool_name(automation_name: &str) -> String {
    format!("{WORKFLOW_TOOL_PREFIX}{automation_name}")
}

/// Strip directories + known suffixes — `path/to/foo.automation.yml` → `foo`.
/// The MCP tool name for an automation: the filename up to its first dot.
///
/// Deliberately NOT `AutomationEntry::name`. That follows the compiler's rule
/// (the YAML `name:`, else the path with its suffix stripped, directories
/// kept), and switching to it would rename every automation tool an agent can
/// see for any automation living in a subdirectory. Tool names are part of the
/// LLM-facing contract, so this rule stays where it is.
fn automation_display_name(file_path: &str) -> String {
    let stem = std::path::Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workflow");
    stem.split('.').next().unwrap_or(stem).to_string()
}

/// JSON schema with a single free-form `variables` object — every automation
/// tool advertises the same shape until per-automation variable schemas are
/// re-introduced.
fn variables_only_schema() -> Map<String, Value> {
    let raw = json!({
        "type": "object",
        "properties": {
            "variables": {
                "type": "object",
                "description": "Optional template variables passed to the automation."
            }
        }
    });
    serde_json::from_value(raw).expect("automation tool schema is well-formed")
}

pub async fn get_all_automation_tools(
    config_manager: ConfigManager<WorkingCopy>,
) -> Result<HashMap<String, OxyTool>, OxyError> {
    let automations = config_manager.list_automations().await?;
    let mut tools_map = HashMap::with_capacity(automations.len());
    for automation in automations {
        let name = automation_display_name(&automation.file_path);
        let tool_name = get_automation_tool_name(&name);
        let tool = Tool::new(tool_name.clone(), "", Arc::new(variables_only_schema()));
        tools_map.insert(
            tool_name,
            OxyTool {
                tool,
                tool_type: ToolType::Automation,
                name,
            },
        );
    }
    Ok(tools_map)
}

pub async fn resolve_automation_tool(
    config_manager: ConfigManager<WorkingCopy>,
    automation_path: PathBuf,
) -> Result<(String, OxyTool), OxyError> {
    let config = config_manager.get_config();
    let relative_path = automation_path
        .strip_prefix(&config.workspace_path)
        .map_err(|_| {
            OxyError::ConfigurationError(format!(
                "Automation path {} is not within project path {}",
                automation_path.display(),
                config.workspace_path.display()
            ))
        })?
        .to_path_buf();
    let name = automation_display_name(&relative_path.to_string_lossy());
    let tool_name = get_automation_tool_name(&name);
    let tool = Tool::new(tool_name.clone(), "", Arc::new(variables_only_schema()));
    Ok((
        tool_name,
        OxyTool {
            tool,
            tool_type: ToolType::Automation,
            name,
        },
    ))
}

pub async fn run_automation_tool(
    workspace_manager: &oxy::adapters::workspace::manager::WorkspaceManager<WorkingCopy>,
    automation_name: String,
    arguments: Option<Map<String, Value>>,
    _filters: Option<oxy::adapters::session_filters::SessionFilters>,
    _connections: Option<oxy::config::model::ConnectionOverrides>,
    meta_variables: HashMap<String, Value>,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    use rmcp::model::{CallToolResult, Content};

    // Find an automation file whose display name matches the requested tool.
    let automations = workspace_manager
        .config_manager
        .list_automations()
        .await
        .map_err(|e| {
            rmcp::ErrorData::internal_error(format!("Failed to list automations: {e}"), None)
        })?;
    let automation_path = automations
        .into_iter()
        .map(|a| std::path::PathBuf::from(a.file_path))
        .find(|p| automation_display_name(&p.to_string_lossy()) == automation_name)
        .ok_or_else(|| {
            rmcp::ErrorData::invalid_request(
                format!("Automation '{automation_name}' not found"),
                None,
            )
        })?;

    let yaml = tokio::fs::read_to_string(&automation_path)
        .await
        .map_err(|e| {
            rmcp::ErrorData::internal_error(
                format!("read {}: {e}", automation_path.display()),
                None,
            )
        })?;
    let automation: agentic_automation::AutomationConfig = serde_yaml::from_str(&yaml)
        .map_err(|e| rmcp::ErrorData::internal_error(format!("parse automation: {e}"), None))?;

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
    let workspace: std::sync::Arc<dyn agentic_automation::WorkspaceContext> = project_ctx;
    let results = agentic_pipeline::automation_run::run_inline_automation_with(
        workspace.as_ref(),
        automation,
        variables,
        None,
    )
    .await
    .map_err(|e| rmcp::ErrorData::internal_error(format!("inline automation: {e}"), None))?;

    let body = serde_json::to_string_pretty(&results).unwrap_or_else(|_| "{}".to_string());
    Ok(CallToolResult::success(vec![Content::text(body)]))
}
