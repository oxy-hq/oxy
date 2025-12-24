//! Agent card generation logic for A2A agent/* methods.
//!
//! This module contains the implementation of agent card generation,
//! which converts Oxy agent configurations to A2A AgentCard format.

use a2a::{
    error::A2aError,
    types::{AgentCard, AgentInterface, AgentSkill, TransportProtocol},
};

use crate::{adapters::project::manager::ProjectManager, config::model::AgentConfig};

/// Generate an A2A agent card from an Oxy agent configuration.
///
/// This function:
/// 1. Loads the Oxy agent configuration
/// 2. Extracts agent metadata (name, description)
/// 3. Maps agent capabilities to A2A skills
/// 4. Constructs endpoint URLs
/// 5. Returns the generated AgentCard
///
/// # Arguments
///
/// * `agent_name` - The A2A name of the agent (URL identifier)
/// * `agent_ref` - The path to the agent configuration file
/// * `base_url` - Base URL for constructing endpoint URLs
/// * `project_manager` - Project manager for loading agent config
///
/// # Returns
///
/// An AgentCard with all the agent's metadata and capabilities.
pub async fn generate_agent_card(
    agent_name: &str,
    agent_ref: &str,
    base_url: &str,
    project_manager: &ProjectManager,
) -> Result<AgentCard, A2aError> {
    tracing::debug!("Generating agent card for agent: {}", agent_name);

    // Load Oxy agent configuration
    let agent_config = load_agent_config(agent_ref, project_manager).await?;

    // Extract description from agent config
    let description = extract_description(&agent_config);

    // Map agent capabilities to A2A skills
    let skills = map_agent_tools_to_skills(&agent_config);

    // Construct endpoint URLs
    let jsonrpc_url = format!("{}/a2a/agents/{}/v1/jsonrpc", base_url, agent_name);
    let http_url = format!("{}/a2a/agents/{}/v1", base_url, agent_name);

    // Create agent card with primary URL (JSON-RPC preferred)
    let mut card = AgentCard::new(agent_name.to_string(), description, jsonrpc_url.clone());

    // Set preferred transport
    card.preferred_transport = Some(TransportProtocol::JsonRpc);

    // Add HTTP as additional interface
    card.additional_interfaces = Some(vec![AgentInterface {
        url: http_url,
        transport: TransportProtocol::JsonRpc, // HTTP+JSON uses JsonRpc transport
    }]);

    // Set capabilities
    card.capabilities.streaming = Some(true);
    card.capabilities.state_transition_history = Some(true);

    // Add skills
    card.skills = skills;

    Ok(card)
}

/// Load the Oxy agent configuration.
async fn load_agent_config(
    agent_ref: &str,
    project_manager: &ProjectManager,
) -> Result<AgentConfig, A2aError> {
    let config_manager = &project_manager.config_manager;

    config_manager
        .resolve_agent(agent_ref)
        .await
        .map_err(|e| A2aError::ServerError(format!("Failed to load agent config: {}", e)))
}

/// Extract description from agent configuration.
///
/// Falls back to a default description if none is provided.
fn extract_description(agent_config: &AgentConfig) -> String {
    if agent_config.description.is_empty() {
        "An Oxy agent".to_string()
    } else {
        agent_config.description.clone()
    }
}

/// Map Oxy agent tools to A2A skills.
///
/// This function converts the agent's tool definitions into A2A Skill objects.
/// Each tool becomes a skill that the agent can perform.
///
/// TODO: This is a placeholder implementation. In the future, we should:
/// - Parse tool definitions to extract input/output schemas
/// - Map tool categories to skill types
/// - Include tool documentation in skill descriptions
fn map_agent_tools_to_skills(_agent_config: &AgentConfig) -> Vec<AgentSkill> {
    let mut skills = Vec::new();

    // For now, create a generic skill representing the agent's capabilities
    // TODO: Parse agent_config.tools and create specific skills for each tool
    let generic_skill = AgentSkill {
        id: "general-assistance".to_string(),
        name: "General Assistance".to_string(),
        description: "General agent capabilities including task execution and data processing"
            .to_string(),
        tags: vec!["general".to_string(), "assistance".to_string()],
        examples: None,
        input_modes: None,
        output_modes: None,
        security: None,
    };

    skills.push(generic_skill);

    // If agent has specific tools, we could add them here
    // For example:
    // if let Some(tools) = &agent_config.tools {
    //     for tool in tools {
    //         let skill = Skill::new(
    //             tool.name.clone(),
    //             tool.description.clone().unwrap_or_default(),
    //         );
    //         skills.push(skill);
    //     }
    // }

    skills
}
