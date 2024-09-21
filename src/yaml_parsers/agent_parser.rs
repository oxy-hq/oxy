use minijinja::Environment;
use serde::Deserialize;
use std::error::Error;
use std::fs;

#[derive(Deserialize, Debug, Clone)]
pub struct AgentConfig {
    pub model: String,
    pub warehouse: String,
    pub scope: String,
    pub instructions: MessagePair,
    pub tools: Vec<String>,
    pub postscript: MessagePair,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MessagePair {
    pub system_message: String,
    pub user_message: String,
}

impl MessagePair {
    pub fn compile(
        &self,
        ctx: minijinja::value::Value,
    ) -> Result<(String, String), Box<dyn Error>> {
        let mut env = Environment::new();
        env.add_template("system", &self.system_message)?;
        env.add_template("user", &self.user_message)?;

        let compiled_system = env.get_template("system")?.render(&ctx)?;
        let compiled_user = env.get_template("user")?.render(&ctx)?;

        Ok((compiled_system, compiled_user))
    }
}

pub fn parse_agent_config(file_path: &str) -> Result<AgentConfig, Box<dyn Error>> {
    let agent_content = fs::read_to_string(file_path)?;
    let agent: AgentConfig = serde_yaml::from_str(&agent_content)?;
    Ok(agent)
}
