use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum ToolConfig {
    #[serde(rename = "execute_sql")]
    ExecuteSQL {
        name: String,
        #[serde(default = "default_sql_tool_description")]
        description: String,
        warehouse: String,
    },
    #[serde(rename = "retrieval")]
    Retrieval {
        name: String,
        #[serde(default = "default_retrieval_tool_description")]
        description: String,
        data: Vec<String>,
    },
}

fn default_sql_tool_description() -> String {
    "Execute the SQL query. If the query is invalid, fix it and run again.
      Output of this tool is a <file_path> used to retrieve the result."
        .to_string()
}

fn default_retrieval_tool_description() -> String {
    "Retrieve the relevant SQL queries to support query generation.".to_string()
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum OutputType {
    #[default]
    Default,
    File,
}

fn default_tools() -> Option<Vec<ToolConfig>> {
    Some(vec![])
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AgentConfig {
    pub model: String,
    pub retrieval: Option<String>,
    pub system_instructions: String,
    #[serde(default = "default_tools")]
    pub tools: Option<Vec<ToolConfig>>,
    #[serde(default)]
    pub output_type: OutputType,
}

pub fn parse_agent_config(file_path: &str) -> anyhow::Result<AgentConfig> {
    let agent_content = fs::read_to_string(file_path)?;
    let agent: AgentConfig = serde_yaml::from_str(&agent_content)?;
    Ok(agent)
}
