//! A2A Configuration Module
//!
//! This module handles loading and validation of A2A configuration from Oxy's config.yml.
//!
//! # Configuration Structure
//!
//! ```yaml
//! a2a:
//!   agents:
//!     - ref: agents/sales-assistant.agent.yml
//!       name: sales-assistant
//!     - ref: agents/data-analyst.agent.yml
//!       name: data-analyst
//! ```
//!
//! # Validation Rules
//!
//! - `ref` must point to a valid, existing agent configuration file
//! - `name` must be a valid URL path segment (alphanumeric, hyphens, underscores)
//! - `name` must be unique within the configuration
//! - Agent names are case-sensitive
//!
//! # Security
//!
//! Only agents explicitly listed in the `a2a.agents` array will be exposed via A2A protocol.
//! If the `a2a` section is missing or empty, no agents are exposed.

use garde::Validate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

use crate::config::validate::ValidationContext;
use crate::errors::OxyError;

/// A2A configuration from config.yml
///
/// This configures which Oxy agents are exposed via the A2A protocol.
/// If this section is absent or empty, no agents are exposed.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Validate)]
#[garde(context(ValidationContext))]
#[derive(Default)]
pub struct A2aConfig {
    /// List of agents to expose via A2A protocol
    #[garde(dive)]
    #[serde(default)]
    pub agents: Vec<A2aAgentConfig>,
}

/// Configuration for a single A2A-exposed agent
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Validate)]
#[garde(context(ValidationContext))]
#[derive(Default)]
pub struct A2aAgentConfig {
    /// Path to the Oxy agent configuration file (relative to project root)
    ///
    /// Example: `agents/sales-assistant.agent.yml`
    #[garde(length(min = 1))]
    #[garde(custom(validate_agent_ref))]
    pub r#ref: String,

    /// A2A agent name used in URL paths
    ///
    /// This name will be used in the A2A endpoint URLs:
    /// - JSON-RPC: `/a2a/agents/{name}/v1/jsonrpc`
    /// - HTTP: `/a2a/agents/{name}/v1`
    /// - Agent Card: `/a2a/agents/{name}/.well-known/agent-card.json`
    ///
    /// Must be a valid URL path segment (alphanumeric, hyphens, underscores only)
    #[garde(length(min = 1, max = 128))]
    #[garde(pattern(r"^[a-zA-Z0-9_-]+$"))]
    pub name: String,
}

impl A2aConfig {
    /// Check if any agents are configured
    pub fn is_enabled(&self) -> bool {
        !self.agents.is_empty()
    }

    /// Get the number of configured agents
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    /// Check if a specific agent name is configured
    pub fn has_agent(&self, name: &str) -> bool {
        self.agents.iter().any(|a| a.name == name)
    }

    /// Get agent config by name
    pub fn get_agent(&self, name: &str) -> Option<&A2aAgentConfig> {
        self.agents.iter().find(|a| a.name == name)
    }

    /// Get all agent names
    pub fn agent_names(&self) -> Vec<&str> {
        self.agents.iter().map(|a| a.name.as_str()).collect()
    }

    /// Validate the configuration
    ///
    /// This performs additional validation beyond what garde provides:
    /// - Ensures agent names are unique
    /// - Validates agent file paths exist (when project_path is provided)
    pub fn validate_config(&self, project_path: Option<&PathBuf>) -> Result<(), OxyError> {
        // Create a dummy ValidationContext for garde validation
        // A2A config doesn't need the full Oxy config for basic validation
        let dummy_config = crate::config::model::Config {
            slack: None,
            defaults: None,
            models: Vec::new(),
            databases: Vec::new(),
            builder_agent: None,
            project_path: project_path.cloned().unwrap_or_default(),
            integrations: Vec::new(),
            mcp: None,
            a2a: None,
        };
        let context = ValidationContext {
            config: dummy_config,
            metadata: None,
        };

        // Run garde validation first
        self.validate_with(&context).map_err(|e| {
            OxyError::ConfigurationError(format!("A2A configuration validation failed: {}", e))
        })?;

        // Check for duplicate agent names
        let mut seen_names = HashSet::new();
        for agent in &self.agents {
            if !seen_names.insert(&agent.name) {
                return Err(OxyError::ConfigurationError(format!(
                    "Duplicate A2A agent name: {}",
                    agent.name
                )));
            }
        }

        // Validate agent file paths exist (if project_path provided)
        if let Some(base_path) = project_path {
            for agent in &self.agents {
                let agent_path = base_path.join(&agent.r#ref);
                if !agent_path.exists() {
                    return Err(OxyError::ConfigurationError(format!(
                        "A2A agent config file not found: {} (resolved to: {})",
                        agent.r#ref,
                        agent_path.display()
                    )));
                }
            }
        }

        Ok(())
    }
}

impl A2aAgentConfig {
    /// Get the full path to the agent config file
    pub fn resolve_path(&self, project_path: &PathBuf) -> PathBuf {
        project_path.join(&self.r#ref)
    }

    /// Check if the agent config file exists
    pub fn exists(&self, project_path: &PathBuf) -> bool {
        self.resolve_path(project_path).exists()
    }
}

/// Validate that the agent ref field is not empty and doesn't contain invalid characters
fn validate_agent_ref(value: &str, _context: &ValidationContext) -> garde::Result {
    if value.is_empty() {
        return Err(garde::Error::new("Agent ref cannot be empty"));
    }

    // Check for path traversal attempts
    if value.contains("..") {
        return Err(garde::Error::new(
            "Agent ref cannot contain path traversal (..)",
        ));
    }

    // Should be a relative path
    if value.starts_with('/') {
        return Err(garde::Error::new("Agent ref must be a relative path"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a2a_config_default() {
        let config = A2aConfig::default();
        assert!(!config.is_enabled());
        assert_eq!(config.agent_count(), 0);
    }

    #[test]
    fn test_a2a_config_with_agents() {
        let config = A2aConfig {
            agents: vec![
                A2aAgentConfig {
                    r#ref: "agents/sales.agent.yml".to_string(),
                    name: "sales".to_string(),
                },
                A2aAgentConfig {
                    r#ref: "agents/analyst.agent.yml".to_string(),
                    name: "analyst".to_string(),
                },
            ],
        };

        assert!(config.is_enabled());
        assert_eq!(config.agent_count(), 2);
        assert!(config.has_agent("sales"));
        assert!(config.has_agent("analyst"));
        assert!(!config.has_agent("unknown"));
    }

    #[test]
    fn test_get_agent() {
        let config = A2aConfig {
            agents: vec![A2aAgentConfig {
                r#ref: "agents/sales.agent.yml".to_string(),
                name: "sales".to_string(),
            }],
        };

        let agent = config.get_agent("sales");
        assert!(agent.is_some());
        assert_eq!(agent.unwrap().r#ref, "agents/sales.agent.yml");

        let missing = config.get_agent("unknown");
        assert!(missing.is_none());
    }

    #[test]
    fn test_agent_names() {
        let config = A2aConfig {
            agents: vec![
                A2aAgentConfig {
                    r#ref: "agents/sales.agent.yml".to_string(),
                    name: "sales".to_string(),
                },
                A2aAgentConfig {
                    r#ref: "agents/analyst.agent.yml".to_string(),
                    name: "analyst".to_string(),
                },
            ],
        };

        let names = config.agent_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"sales"));
        assert!(names.contains(&"analyst"));
    }

    #[test]
    fn test_validate_duplicate_names() {
        let config = A2aConfig {
            agents: vec![
                A2aAgentConfig {
                    r#ref: "agents/sales.agent.yml".to_string(),
                    name: "sales".to_string(),
                },
                A2aAgentConfig {
                    r#ref: "agents/sales2.agent.yml".to_string(),
                    name: "sales".to_string(), // Duplicate name
                },
            ],
        };

        let result = config.validate_config(None);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Duplicate A2A agent name")
        );
    }

    #[test]
    fn test_validate_agent_ref() {
        let dummy_config = crate::config::model::Config {
            defaults: None,
            models: Vec::new(),
            databases: Vec::new(),
            builder_agent: None,
            project_path: PathBuf::default(),
            integrations: Vec::new(),
            mcp: None,
            a2a: None,
            slack: None,
        };
        let context = ValidationContext {
            config: dummy_config,
            metadata: None,
        };

        // Valid refs
        assert!(validate_agent_ref("agents/sales.agent.yml", &context).is_ok());
        assert!(validate_agent_ref("agents/subfolder/analyst.agent.yml", &context).is_ok());

        // Invalid refs
        assert!(validate_agent_ref("", &context).is_err());
        assert!(validate_agent_ref("../etc/passwd", &context).is_err());
        assert!(validate_agent_ref("/absolute/path", &context).is_err());
    }

    #[test]
    fn test_agent_config_resolve_path() {
        let agent = A2aAgentConfig {
            r#ref: "agents/sales.agent.yml".to_string(),
            name: "sales".to_string(),
        };

        let project_path = PathBuf::from("/project");
        let resolved = agent.resolve_path(&project_path);
        assert_eq!(resolved, PathBuf::from("/project/agents/sales.agent.yml"));
    }

    #[test]
    fn test_agent_name_validation() {
        let dummy_config = crate::config::model::Config {
            slack: None,
            defaults: None,
            models: Vec::new(),
            databases: Vec::new(),
            builder_agent: None,
            project_path: PathBuf::default(),
            integrations: Vec::new(),
            mcp: None,
            a2a: None,
        };
        let context = ValidationContext {
            config: dummy_config,
            metadata: None,
        };

        // Valid names
        let valid = A2aAgentConfig {
            r#ref: "agents/sales.agent.yml".to_string(),
            name: "sales-assistant".to_string(),
        };
        assert!(valid.validate_with(&context).is_ok());

        let valid2 = A2aAgentConfig {
            r#ref: "agents/sales.agent.yml".to_string(),
            name: "data_analyst_v2".to_string(),
        };
        assert!(valid2.validate_with(&context).is_ok());

        // Invalid names (with special characters)
        let invalid = A2aAgentConfig {
            r#ref: "agents/sales.agent.yml".to_string(),
            name: "sales assistant".to_string(), // Space not allowed
        };
        assert!(invalid.validate_with(&context).is_err());

        let invalid2 = A2aAgentConfig {
            r#ref: "agents/sales.agent.yml".to_string(),
            name: "sales/assistant".to_string(), // Slash not allowed
        };
        assert!(invalid2.validate_with(&context).is_err());
    }
}
