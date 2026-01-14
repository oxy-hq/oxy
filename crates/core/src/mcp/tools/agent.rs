use std::collections::HashMap;
use std::sync::Arc;

use rmcp::model::Tool;
use serde_json::{Map, Value};

use crate::adapters::session_filters::SessionFilters;
use crate::config::ConfigManager;
use crate::errors::OxyError;
use crate::service::agent::{get_agent_config, list_agents};

use crate::mcp::types::{AGENT_TOOL_PREFIX, AgentToolInput, OxyTool, ToolType};

pub fn get_agent_tool_name(agent_name: &str) -> String {
    format!("{AGENT_TOOL_PREFIX}{agent_name}")
}

/// Gets all agent tools from the project
pub async fn get_all_agent_tools(
    config_manager: ConfigManager,
) -> Result<HashMap<String, OxyTool>, OxyError> {
    let mut tools_map = HashMap::new();
    for agent in list_agents(config_manager.clone()).await? {
        let agent_config = get_agent_config(config_manager.clone(), agent.clone()).await?;

        let schema = schemars::schema_for!(AgentToolInput);
        let schema_json = serde_json::to_value(schema)?;

        let tool_name = get_agent_tool_name(agent_config.name.as_str());
        let tool = Tool::new(
            tool_name.clone(),
            agent_config.description,
            Arc::new(serde_json::from_value(schema_json)?),
        );
        let oxy_tool = OxyTool {
            tool,
            tool_type: ToolType::Agent,
            name: agent_config.name.to_owned(),
        };
        tools_map.insert(tool_name, oxy_tool);
    }
    Ok(tools_map)
}

/// Gets an agent tool from a specific file path
pub async fn resolve_agent_tool(
    config_manager: ConfigManager,
    agent_path: std::path::PathBuf,
) -> Result<(String, OxyTool), OxyError> {
    // Convert absolute path to relative path from project root
    let config = config_manager.get_config();
    let relative_path = agent_path
        .strip_prefix(&config.project_path)
        .map_err(|_| {
            OxyError::ConfigurationError(format!(
                "Agent path {} is not within project path {}",
                agent_path.display(),
                config.project_path.display()
            ))
        })?
        .to_str()
        .ok_or_else(|| {
            OxyError::ConfigurationError(format!(
                "Failed to convert agent path to string: {}",
                agent_path.display()
            ))
        })?
        .to_string();

    // Get agent config
    let agent_config = get_agent_config(config_manager.clone(), relative_path.clone()).await?;

    let schema = schemars::schema_for!(AgentToolInput);
    let schema_json = serde_json::to_value(schema)?;

    let tool_name = get_agent_tool_name(agent_config.name.as_str());
    let tool = Tool::new(
        tool_name.clone(),
        agent_config.description,
        Arc::new(serde_json::from_value(schema_json)?),
    );

    let oxy_tool = OxyTool {
        tool,
        tool_type: ToolType::Agent,
        name: agent_config.name.to_owned(),
    };

    tracing::debug!(
        "Created agent tool '{}' from file: {}",
        tool_name,
        agent_path.display()
    );

    Ok((tool_name, oxy_tool))
}

/// Runs an agent tool with the given arguments
pub async fn run_agent_tool(
    project_manager: &crate::adapters::project::manager::ProjectManager,
    agent_name: String,
    arguments: Option<Map<String, Value>>,
    filters: Option<SessionFilters>,
    connections: Option<crate::config::model::ConnectionOverrides>,
    meta_variables: std::collections::HashMap<String, Value>,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    use rmcp::model::{CallToolResult, Content};

    match arguments {
        None => Err(rmcp::ErrorData::invalid_request(
            "Missing 'arguments' parameter".to_string(),
            None,
        )),
        Some(args) => {
            let question = args.get("question").and_then(|v| v.as_str()).ok_or(
                rmcp::ErrorData::invalid_request("Missing 'question' parameter".to_string(), None),
            )?;

            // Extract variables from arguments (if any)
            let arg_variables = args
                .get("variables")
                .and_then(|v| v.as_object())
                .map(|obj| {
                    obj.iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect::<std::collections::HashMap<_, _>>()
                })
                .unwrap_or_default();

            let config_manager = project_manager.config_manager.clone();
            let agent_path =
                crate::service::agent::get_path_by_name(config_manager, agent_name.clone())
                    .await
                    .map_err(|e| {
                        rmcp::ErrorData::internal_error(
                            format!("Failed to get agent path: {e}"),
                            None,
                        )
                    })?;

            // For now, we'll use an empty HashMap as defaults
            let default_variables = std::collections::HashMap::new();

            // Merge variables using proper precedence: defaults < arguments < meta
            let merged_variables = crate::mcp::variables::merge_variables(
                default_variables,
                meta_variables,
                arg_variables,
            );

            // Run the agent with filters, connection overrides, and merged variables
            let output = crate::service::agent::run_agent(
                project_manager.clone(),
                &agent_path,
                question.to_string(),
                crate::execute::writer::NoopHandler,
                vec![],
                filters,
                connections,
                None, // No globals
                Some(merged_variables),
                Some(crate::service::agent::ExecutionSource::Mcp {
                    session_id: None, // MCP doesn't have session tracking yet
                }),
                None,
            )
            .await
            .map_err(|e| {
                rmcp::ErrorData::internal_error(format!("Failed to run agent: {e}"), None)
            })?;

            Ok(CallToolResult::success(vec![Content::text(
                output.to_string(),
            )]))
        }
    }
}
