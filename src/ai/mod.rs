pub mod agent;
pub mod prompt;
pub mod retrieval;
pub mod toolbox;
pub mod tools;

use crate::yaml_parsers::config_parser::{get_config_path, parse_config};
use agent::{LLMAgent, OpenAIAgent};
use prompt::PromptBuilder;
use std::path::PathBuf;
use toolbox::ToolBox;

pub async fn setup_agent(
    agent_name: Option<&str>,
) -> Result<(Box<dyn LLMAgent + Send>, PathBuf), Box<dyn std::error::Error>> {
    let config_path = get_config_path();
    let config = parse_config(config_path)?;
    let parsed_config = config.load_config(agent_name.filter(|s| !s.is_empty()))?;
    let project_path = PathBuf::from(&config.defaults.project_path);
    let mut tools = ToolBox::default();
    let mut prompt_builder = PromptBuilder::new(&parsed_config.agent_config, &project_path);
    prompt_builder.setup(&parsed_config.warehouse).await;
    tools.fill_toolbox(&parsed_config, &prompt_builder).await;
    // Create the agent from the parsed config and entity config
    let agent = OpenAIAgent::new(parsed_config, tools, prompt_builder);
    Ok((Box::new(agent), project_path))
}
