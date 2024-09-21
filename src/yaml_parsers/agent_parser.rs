use serde::Deserialize;
use std::error::Error;
use std::fs;

#[derive(Deserialize, Debug)]
pub struct AgentConfig {
    pub model: String,
    pub warehouse: String,
    pub instructions: String,
    pub scope: String,
}

pub fn parse_agent_config(file_path: &str) -> Result<AgentConfig, Box<dyn Error>> {
    let agent_content = fs::read_to_string(file_path)?;
    let agent: AgentConfig = serde_yaml::from_str(&agent_content)?;
    Ok(agent)
}
