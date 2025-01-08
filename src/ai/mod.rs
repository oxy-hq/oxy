pub mod agent;
pub mod anonymizer;
pub mod retrieval;
pub mod toolbox;
pub mod tools;
pub mod utils;

use std::{path::PathBuf, sync::Arc};

use crate::{
    config::{
        load_config,
        model::{AgentConfig, AnonymizerConfig, Config, FileFormat, Model, ToolConfig},
    },
    errors::OnyxError,
    execute::agent::ToolCall,
    union_tools,
};
use agent::OpenAIAgent;
use anonymizer::{base::Anonymizer, flash_text::FlashTextAnonymizer};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use toolbox::ToolBox;
use tools::{ExecuteSQLParams, ExecuteSQLTool, RetrieveParams, RetrieveTool, Tool};

pub fn setup_agent(
    agent_file: Option<&PathBuf>,
    file_format: &FileFormat,
) -> Result<(OpenAIAgent, AgentConfig, Config), OnyxError> {
    let config = load_config()?;

    let (agent_config, agent_name) = config.load_agent_config(agent_file)?;
    let agent = from_config(&agent_name, &config, &agent_config, file_format)
        .map_err(|e| OnyxError::AgentError(format!("Error setting up agent: {}", e)))?;
    Ok((agent, agent_config, config))
}

pub fn from_config(
    agent_name: &str,
    config: &Config,
    agent_config: &AgentConfig,
    file_format: &FileFormat,
) -> anyhow::Result<OpenAIAgent> {
    let model = config.find_model(&agent_config.model).unwrap();
    let anonymizer: Option<Box<dyn Anonymizer + Send + Sync>> = match &agent_config.anonymize {
        None => None,
        Some(AnonymizerConfig::FlashText {
            source,
            pluralize,
            case_sensitive,
        }) => {
            let mut anonymizer = FlashTextAnonymizer::new(pluralize, case_sensitive);
            anonymizer.add_keywords_file(source)?;
            Some(Box::new(anonymizer))
        }
    };
    let toolbox = Arc::new(tools_from_config(&agent_name, config, agent_config));

    match model {
        Model::OpenAI {
            name: _,
            model_ref,
            key_var,
            api_url,
            azure_deployment_id,
            azure_api_version,
        } => {
            let api_key = std::env::var(&key_var).unwrap_or_else(|_| {
                panic!("OpenAI key not found in environment variable {}", key_var)
            });
            Ok(OpenAIAgent::new(
                model_ref,
                api_url,
                api_key,
                azure_deployment_id,
                azure_api_version,
                agent_config.system_instructions.to_string(),
                agent_config.output_format.clone(),
                anonymizer,
                file_format.clone(),
                toolbox.clone(),
            ))
        }
        Model::Ollama {
            name: _,
            model_ref,
            api_key,
            api_url,
        } => Ok(OpenAIAgent::new(
            model_ref,
            Some(api_url),
            api_key,
            None,
            None,
            agent_config.system_instructions.to_string(),
            agent_config.output_format.clone(),
            anonymizer,
            file_format.clone(),
            toolbox.clone(),
        )),
    }
}

fn tools_from_config(
    agent_name: &str,
    config: &Config,
    agent_config: &AgentConfig,
) -> ToolBox<MultiTool> {
    let mut toolbox = ToolBox::new();
    for tool_config in agent_config.tools.iter() {
        match tool_config {
            ToolConfig::ExecuteSQL(execute_sql) => {
                let warehouse_config = config
                    .find_warehouse(&execute_sql.warehouse)
                    .unwrap_or_else(|_| panic!("Warehouse {} not found", &execute_sql.warehouse));
                let tool: ExecuteSQLTool = ExecuteSQLTool {
                    config: warehouse_config.clone(),
                    tool_description: execute_sql.description.to_string(),
                    output_format: agent_config.output_format.clone(),
                };
                toolbox.add_tool(execute_sql.name.to_string(), tool.into());
            }
            ToolConfig::Retrieval(retrieval) => {
                let tool = RetrieveTool::new(agent_name, &retrieval);
                toolbox.add_tool(retrieval.name.to_string(), tool.into());
            }
        };
    }
    toolbox
}

union_tools!(
    MultiTool,
    MultiToolInput,
    ExecuteSQLTool,
    ExecuteSQLParams,
    RetrieveTool,
    RetrieveParams
);
