use serde::Deserialize;
use std::fs;
use std::error::Error;

#[derive(Deserialize, Debug)]
pub struct Agent {
    pub model: String,
    pub warehouse: String,
    pub instructions: String,
    pub scope: String,
}

pub fn parse_agent_config(file_path: &str) -> Result<Agent, Box<dyn Error>> {
    let agent_content = fs::read_to_string(file_path)?;
    let agent: Agent = serde_yaml::from_str(&agent_content)?;
    Ok(agent)
}
