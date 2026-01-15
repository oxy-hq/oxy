use std::collections::HashMap;
use std::path::Path;

use crate::integrations::mcp::ToolType;
use oxy::config::ConfigManager;
use oxy_shared::errors::OxyError;

use super::{agent, semantic, sql, workflow};
use crate::integrations::mcp::types::OxyTool;

/// Handles resolution of MCP tools from different resource types
struct McpToolResolver {
    config_manager: ConfigManager,
}

impl McpToolResolver {
    fn new(config_manager: ConfigManager) -> Self {
        Self { config_manager }
    }

    /// Resolves a tool for a given path based on its resource type
    async fn resolve_tool(&self, path: &Path) -> Result<Option<(String, OxyTool)>, OxyError> {
        match detect_resource_type(path) {
            Some(ToolType::Agent) => {
                agent::resolve_agent_tool(self.config_manager.clone(), path.to_path_buf())
                    .await
                    .map(Some)
            }
            Some(ToolType::Workflow) => {
                workflow::resolve_workflow_tool(self.config_manager.clone(), path.to_path_buf())
                    .await
                    .map(Some)
            }
            Some(ToolType::SemanticTopic) => {
                semantic::resolve_semantic_tool(self.config_manager.clone(), path.to_path_buf())
                    .await
                    .map(Some)
            }
            Some(ToolType::SqlFile) => {
                sql::resolve_execute_sql_tool(self.config_manager.clone(), path.to_path_buf())
                    .await
                    .map(Some)
            }
            None => {
                tracing::warn!("Unsupported file type: {}, skipping", path.display());
                Ok(None)
            }
        }
    }
}

/// Discovers and loads MCP tools based on the configuration strategy.
///
/// # Configuration Strategies
///
/// 1. **Explicit MCP Configuration**: When `mcp.tools` is defined in config.yml,
///    only the specified patterns are used to discover tools. This provides fine-grained
///    control over which resources are exposed via MCP.
///
/// 2. **Default Behavior**: When no MCP configuration exists, all agents and workflows
///    in the project are automatically exposed as MCP tools.
///
/// # Returns
///
/// A HashMap mapping tool names to their OxyTool definitions. Returns empty map if
/// explicit configuration exists but matches no files.
pub async fn get_mcp_tools(
    config_manager: ConfigManager,
) -> Result<HashMap<String, OxyTool>, OxyError> {
    // Clone resource patterns if MCP config exists to avoid borrowing issues
    let resource_patterns = {
        let config = config_manager.get_config();
        config.mcp.as_ref().map(|mcp| mcp.tools.clone())
    };

    // Determine discovery strategy based on MCP configuration presence
    match resource_patterns {
        Some(patterns) => {
            tracing::info!(
                "Using explicit MCP configuration with {} resource patterns",
                patterns.len()
            );

            if patterns.is_empty() {
                tracing::info!("MCP resource patterns empty - no tools will be exposed");
                return Ok(HashMap::new());
            }

            get_tools(config_manager, &patterns).await
        }
        None => {
            tracing::info!(
                "No MCP configuration found - using default discovery (all agents and workflows)"
            );
            get_default_tools(config_manager).await
        }
    }
}

/// Default discovery strategy: exposes all agents and workflows as MCP tools
async fn get_default_tools(
    config_manager: ConfigManager,
) -> Result<HashMap<String, OxyTool>, OxyError> {
    let mut tools_map = agent::get_all_agent_tools(config_manager.clone()).await?;
    tools_map.extend(workflow::get_all_workflow_tools(config_manager.clone()).await?);

    tracing::debug!("Discovered MCP tools: {:?}", tools_map.keys());
    Ok(tools_map)
}

/// Discovers tools by resolving glob patterns from explicit MCP configuration
async fn get_tools(
    config_manager: ConfigManager,
    patterns: &[String],
) -> Result<HashMap<String, OxyTool>, OxyError> {
    let config = config_manager.get_config();
    let base_path = &config.project_path;
    let mut tools_map = HashMap::new();

    let resolver = McpToolResolver::new(config_manager.clone());

    for pattern in patterns {
        let full_pattern = base_path.join(pattern);
        let pattern_str = full_pattern.to_str().ok_or_else(|| {
            OxyError::from(anyhow::anyhow!(
                "Invalid UTF-8 in pattern path: {}",
                full_pattern.display()
            ))
        })?;

        match glob::glob(pattern_str) {
            Ok(entries) => {
                for entry in entries {
                    match entry {
                        Ok(path) if path.is_file() => {
                            if let Some((tool_name, oxy_tool)) =
                                resolver.resolve_tool(&path).await?
                            {
                                tools_map.insert(tool_name, oxy_tool);
                            }
                        }
                        Ok(_) => {} // Skip non-files
                        Err(e) => {
                            tracing::warn!(
                                "Error reading glob entry for pattern '{}': {}",
                                pattern,
                                e
                            );
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Invalid glob pattern '{}': {}", pattern, e);
            }
        }
    }

    tracing::info!(
        "Created {} MCP tools from explicit configuration",
        tools_map.len()
    );
    tracing::debug!("MCP tool names: {:?}", tools_map.keys().collect::<Vec<_>>());

    Ok(tools_map)
}

/// Detects the resource type based on file extension
pub fn detect_resource_type(path: &Path) -> Option<ToolType> {
    let file_name = path.file_name()?.to_str()?;

    if file_name.ends_with(".agent.yml") || file_name.ends_with(".agent.yaml") {
        Some(ToolType::Agent)
    } else if file_name.ends_with(".workflow.yml")
        || file_name.ends_with(".workflow.yaml")
        || file_name.ends_with(".automation.yml")
        || file_name.ends_with(".automation.yaml")
    {
        Some(ToolType::Workflow)
    } else if file_name.ends_with(".topic.yml") || file_name.ends_with(".topic.yaml") {
        Some(ToolType::SemanticTopic)
    } else if file_name.ends_with(".sql") {
        Some(ToolType::SqlFile)
    } else {
        None
    }
}
