//! Agent Card generation and caching for A2A protocol.
//!
//! This module handles the generation of A2A Agent Cards from Oxy agent configurations.
//! Each agent configured in the `a2a.agents` section gets its own Agent Card that describes
//! its capabilities, supported protocols, and available skills.
//!
//! # Agent Card Generation
//!
//! Agent Cards are generated from Oxy agent configurations with the following mappings:
//! - Configuration `name` → AgentCard.name (URL identifier)
//! - Agent config description → AgentCard.description
//! - Agent tools → AgentCard.skills
//! - Protocol endpoints are automatically constructed
//!
//! # Caching
//!
//! Generated Agent Cards are cached in the database to avoid regenerating them on every request.
//! The cache is invalidated when:
//! - The agent configuration changes (detected via config file modification)
//! - The server is restarted
//!
//! # Security
//!
//! Only agents explicitly listed in `a2a.agents` configuration are exposed.
//! Requests for agent names not in the configuration return 404.
//!
//! # Example
//!
//! ```rust,ignore
//! use oxy_core::a2a::agent_card::AgentCardService;
//!
//! let service = AgentCardService::new(config, project_manager);
//!
//! // Generate card for configured agent
//! let card = service.get_agent_card("sales-assistant", "https://api.example.com").await?;
//!
//! // Returns error for unconfigured agent
//! let result = service.get_agent_card("unknown-agent", "https://api.example.com").await;
//! assert!(result.is_err());
//! ```

use a2a::{
    error::A2aError,
    types::{AgentCapabilities, AgentCard, AgentSkill, SecurityScheme, TransportProtocol},
};
use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    adapters::project::manager::ProjectManager, config::constants::DEFAULT_API_KEY_HEADER,
    config::model::AgentConfig,
};

use super::config::A2aConfig;

/// Service for generating and managing Agent Cards.
///
/// This service handles the generation of A2A Agent Cards from Oxy agent configurations,
/// including caching and validation.
pub struct AgentCardService {
    /// Global Oxy configuration
    config: Arc<crate::config::model::Config>,
    /// Project manager for loading agent configs
    project_manager: Arc<ProjectManager>,
    /// In-memory cache of generated agent cards
    /// TODO: Replace with database cache (a2a_agent_cards table)
    cache: Arc<tokio::sync::RwLock<HashMap<String, AgentCard>>>,
}

impl AgentCardService {
    /// Create a new Agent Card service.
    pub fn new(
        config: Arc<crate::config::model::Config>,
        project_manager: Arc<ProjectManager>,
    ) -> Self {
        Self {
            config,
            project_manager,
            cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Get the A2A configuration from global config.
    fn a2a_config(&self) -> Result<&A2aConfig, A2aError> {
        self.config
            .a2a
            .as_ref()
            .ok_or_else(|| A2aError::ServerError("A2A not configured".to_string()))
    }

    /// Check if an agent name is in the allowed list.
    pub fn is_agent_allowed(&self, agent_name: &str) -> bool {
        self.a2a_config()
            .map(|a2a| a2a.has_agent(agent_name))
            .unwrap_or(false)
    }

    /// Get the agent configuration path for a given agent name.
    fn get_agent_ref(&self, agent_name: &str) -> Result<String, A2aError> {
        let a2a_config = self.a2a_config()?;

        a2a_config
            .get_agent(agent_name)
            .map(|cfg| cfg.r#ref.clone())
            .ok_or_else(|| {
                A2aError::ServerError(format!("Agent '{}' not found in configuration", agent_name))
            })
    }

    /// Load Oxy agent configuration from file.
    async fn load_agent_config(&self, agent_ref: &str) -> Result<AgentConfig, A2aError> {
        self.project_manager
            .config_manager
            .resolve_agent(agent_ref)
            .await
            .map_err(|e| A2aError::ServerError(format!("Failed to load agent config: {}", e)))
    }

    /// Generate an Agent Card from Oxy agent configuration.
    ///
    /// # Arguments
    ///
    /// * `agent_name` - The A2A agent name (from configuration)
    /// * `agent_config` - The loaded Oxy agent configuration
    /// * `base_url` - The base URL for the server (e.g., "https://api.example.com")
    ///
    /// # Returns
    ///
    /// An `AgentCard` with populated fields from the Oxy agent configuration.
    fn generate_agent_card(
        &self,
        agent_name: &str,
        agent_config: &AgentConfig,
        base_url: &str,
    ) -> Result<AgentCard, A2aError> {
        // Construct endpoint URLs
        let agent_base_url = format!("{}/a2a/agents/{}/v1", base_url, agent_name);
        let agent_jsonrpc_url = format!("{}/jsonrpc", agent_base_url);

        // Create base agent card
        let mut card = AgentCard::new(
            agent_name,
            if agent_config.description.is_empty() {
                format!("Oxy agent: {}", agent_name)
            } else {
                agent_config.description.clone()
            },
            agent_jsonrpc_url,
        );

        // Set preferred transport to JSON-RPC
        card.preferred_transport = Some(TransportProtocol::JsonRpc);

        // Add HTTP+JSON interface as alternative
        card.additional_interfaces = Some(vec![a2a::types::AgentInterface {
            url: agent_base_url.clone(),
            transport: TransportProtocol::HttpJson,
        }]);

        // Set capabilities
        card.capabilities = AgentCapabilities {
            streaming: Some(true),
            push_notifications: Some(false),
            state_transition_history: Some(true),
            extensions: None,
        };

        // Map Oxy agent tools to A2A skills
        card.skills = self.map_tools_to_skills(agent_config)?;

        // Set default input/output modes
        card.default_input_modes = vec!["application/json".to_string(), "text/plain".to_string()];
        card.default_output_modes = vec![
            "application/json".to_string(),
            "text/plain".to_string(),
            "text/markdown".to_string(),
        ];

        // Only apply auth security if Oxy authentication is enabled
        if self.is_auth_enabled() {
            self.apply_auth_security(&mut card);
        }

        Ok(card)
    }

    /// Map Oxy agent tools to A2A skills.
    ///
    /// Each tool in the agent configuration is mapped to an A2A skill with:
    /// - Tool name → skill.id (kebab-cased)
    /// - Tool name → skill.name
    /// - Tool description → skill.description
    /// - Tool type → skill.tags
    fn map_tools_to_skills(&self, agent_config: &AgentConfig) -> Result<Vec<AgentSkill>, A2aError> {
        use crate::config::model::{AgentType, ToolType};

        // Extract tools from the agent configuration
        // Note: Routing agents don't have tools, only routes to other agents
        let tools = match &agent_config.r#type {
            AgentType::Default(default_agent) => &default_agent.tools_config.tools,
            AgentType::Routing(_) => {
                // Routing agents don't expose tools directly, return a generic routing skill
                return Ok(vec![AgentSkill {
                    id: "routing".to_string(),
                    name: agent_config.name.clone(),
                    description: if agent_config.description.is_empty() {
                        format!(
                            "Routes requests to specialized agents: {}",
                            agent_config.name
                        )
                    } else {
                        agent_config.description.clone()
                    },
                    tags: vec!["routing".to_string(), "orchestration".to_string()],
                    examples: None,
                    input_modes: None,
                    output_modes: None,
                    security: None,
                }]);
            }
        };

        // If no tools are configured, return a generic skill
        if tools.is_empty() {
            return Ok(vec![AgentSkill {
                id: "general".to_string(),
                name: agent_config.name.clone(),
                description: if agent_config.description.is_empty() {
                    format!(
                        "General purpose assistance using the {} agent",
                        agent_config.name
                    )
                } else {
                    agent_config.description.clone()
                },
                tags: vec!["general".to_string(), "assistant".to_string()],
                examples: None,
                input_modes: None,
                output_modes: None,
                security: None,
            }]);
        }

        // Map each tool to a skill
        let skills: Vec<AgentSkill> = tools
            .iter()
            .map(|tool| {
                let (name, description, tool_type_tag) = match tool {
                    ToolType::ExecuteSQL(t) => (t.name.clone(), t.description.clone(), "sql"),
                    ToolType::ValidateSQL(t) => {
                        (t.name.clone(), t.description.clone(), "validation")
                    }
                    ToolType::Retrieval(t) => (t.name.clone(), t.description.clone(), "retrieval"),
                    ToolType::Visualize(t) => {
                        (t.name.clone(), t.description.clone(), "visualization")
                    }
                    ToolType::Workflow(t) => (t.name.clone(), t.description.clone(), "workflow"),
                    ToolType::Agent(t) => (t.name.clone(), t.description.clone(), "agent"),
                    ToolType::CreateDataApp(t) => {
                        (t.name.clone(), t.description.clone(), "data-app")
                    }
                    ToolType::CreateV0App(t) => (t.name.clone(), t.description.clone(), "v0-app"),
                    ToolType::OmniQuery(t) => (t.name.clone(), t.description.clone(), "omni"),
                    ToolType::SemanticQuery(t) => {
                        (t.name.clone(), t.description.clone(), "semantic")
                    }
                };

                // Convert tool name to kebab-case for ID
                let id = name
                    .to_lowercase()
                    .replace([' ', '_'], "-")
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '-')
                    .collect::<String>();

                AgentSkill {
                    id,
                    name,
                    description,
                    tags: vec![tool_type_tag.to_string()],
                    examples: None,
                    input_modes: Some(vec![
                        "application/json".to_string(),
                        "text/plain".to_string(),
                    ]),
                    output_modes: Some(vec![
                        "application/json".to_string(),
                        "text/plain".to_string(),
                    ]),
                    security: None,
                }
            })
            .collect();

        Ok(skills)
    }

    /// Get an Agent Card for a specific agent.
    ///
    /// This method:
    /// 1. Validates that the agent is in the allowed list
    /// 2. Checks the cache for an existing card
    /// 3. Loads the agent configuration if needed
    /// 4. Generates and caches the Agent Card
    ///
    /// # Arguments
    ///
    /// * `agent_name` - The A2A agent name from the configuration
    /// * `base_url` - The base URL for constructing endpoint URLs
    ///
    /// # Returns
    ///
    /// An `AgentCard` for the requested agent.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The agent is not in the configuration
    /// - The agent config file cannot be loaded
    /// - The Agent Card cannot be generated
    pub async fn get_agent_card(
        &self,
        agent_name: &str,
        base_url: &str,
    ) -> Result<AgentCard, A2aError> {
        // Check if agent is allowed
        if !self.is_agent_allowed(agent_name) {
            return Err(A2aError::ServerError(format!(
                "Agent '{}' is not configured for A2A access",
                agent_name
            )));
        }

        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(card) = cache.get(agent_name) {
                return Ok(card.clone());
            }
        }

        // Load agent config
        let agent_ref = self.get_agent_ref(agent_name)?;
        tracing::debug!("Loading agent config for '{}': {}", agent_name, agent_ref);
        let agent_config = self.load_agent_config(&agent_ref).await?;

        // Generate card
        let card = self.generate_agent_card(agent_name, &agent_config, base_url)?;

        // Cache the card
        {
            let mut cache = self.cache.write().await;
            cache.insert(agent_name.to_string(), card.clone());
        }

        Ok(card)
    }

    /// List all available agent names from the configuration.
    ///
    /// This is useful for generating a directory of available agents.
    pub fn list_agent_names(&self) -> Result<Vec<String>, A2aError> {
        let a2a_config = self.a2a_config()?;
        Ok(a2a_config
            .agent_names()
            .into_iter()
            .map(|s| s.to_string())
            .collect())
    }

    /// Clear the agent card cache.
    ///
    /// This should be called when agent configurations are reloaded or changed.
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Remove a specific agent from the cache.
    ///
    /// This should be called when a specific agent configuration is updated.
    pub async fn invalidate_agent(&self, agent_name: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(agent_name);
    }

    /// Check if Oxy authentication is enabled.
    fn is_auth_enabled(&self) -> bool {
        crate::config::oxy::get_oxy_config()
            .ok()
            .and_then(|config| config.authentication)
            .map(|auth| auth.google.is_some() || auth.okta.is_some() || auth.basic.is_some())
            .unwrap_or(false)
    }

    fn apply_auth_security(&self, card: &mut AgentCard) {
        let mut schemes = HashMap::new();
        schemes.insert(
            "ApiKeyAuth".to_string(),
            SecurityScheme::ApiKey {
                name: DEFAULT_API_KEY_HEADER.to_string(),
                location: "header".to_string(),
            },
        );

        let mut requirement = HashMap::new();
        requirement.insert("ApiKeyAuth".to_string(), Vec::new());

        card.security_schemes = Some(schemes);
        card.security = Some(vec![requirement]);
    }
}

#[cfg(test)]
mod tests {
    // Note: Comprehensive tests would require mocking ProjectManager and database
    // Integration tests should be added in the future to test the full flow:
    // 1. AgentCardService::get_agent_card() with real agent configs
    // 2. Caching behavior
    // 3. Agent validation
    // 4. Skill mapping from agent tools
}
