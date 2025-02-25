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
        model::{
            AgentConfig, AnonymizerConfig, Config, FileFormat, Model, OutputFormat, ToolConfig,
        },
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
    config: &Config,
) -> Result<(OpenAIAgent, AgentConfig), OnyxError> {
    let (agent_config, agent_name) = config.load_agent_config(agent_file)?;
    let agent = from_config(&agent_name, config, &agent_config, file_format)
        .map_err(|e| OnyxError::AgentError(format!("Error setting up agent: {}", e)))?;
    Ok((agent, agent_config))
}

pub fn setup_eval_agent(prompt: &str, model: &str) -> Result<OpenAIAgent, OnyxError> {
    let config = load_config(None)?;
    let model = config.find_model(model)?;
    let agent = build_agent(
        &model,
        &FileFormat::Json,
        &OutputFormat::Default,
        prompt,
        Arc::new(ToolBox::new()),
        None,
    );
    Ok(agent)
}

fn from_config(
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
            let mut anonymizer = FlashTextAnonymizer::new(pluralize, case_sensitive, config);
            anonymizer.add_keywords_file(source)?;
            Some(Box::new(anonymizer))
        }
    };
    let toolbox = Arc::new(tools_from_config(agent_name, config, agent_config));
    let agent = build_agent(
        &model,
        file_format,
        &agent_config.output_format,
        &agent_config.system_instructions,
        toolbox,
        anonymizer,
    );
    Ok(agent)
}

fn build_agent(
    model: &Model,
    file_format: &FileFormat,
    output_format: &OutputFormat,
    system_instructions: &str,
    tools: Arc<ToolBox<MultiTool>>,
    anonymizer: Option<Box<dyn Anonymizer + Send + Sync>>,
) -> OpenAIAgent {
    match model {
        Model::OpenAI {
            name: _,
            model_ref,
            key_var,
            api_url,
            azure_deployment_id,
            azure_api_version,
        } => {
            let api_key = std::env::var(key_var).unwrap_or_else(|_| {
                panic!("OpenAI key not found in environment variable {}", key_var)
            });
            OpenAIAgent::new(
                model_ref.to_string(),
                api_url.clone(),
                api_key,
                azure_deployment_id.clone(),
                azure_api_version.clone(),
                system_instructions.to_string(),
                output_format.clone(),
                anonymizer,
                file_format.clone(),
                tools,
            )
        }
        Model::Ollama {
            name: _,
            model_ref,
            api_key,
            api_url,
        } => OpenAIAgent::new(
            model_ref.to_string(),
            Some(api_url.clone()),
            api_key.clone(),
            None,
            None,
            system_instructions.to_string(),
            output_format.clone(),
            anonymizer,
            file_format.clone(),
            tools,
        ),
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
            ToolConfig::ExecuteSQL(sql_tool) => {
                let warehouse_config = config
                    .find_warehouse(&sql_tool.warehouse)
                    .unwrap_or_else(|_| panic!("Warehouse {} not found", &sql_tool.warehouse));
                let tool: ExecuteSQLTool = ExecuteSQLTool {
                    warehouse_config: warehouse_config.clone(),
                    tool_name: sql_tool.name.to_string(),
                    tool_description: sql_tool.description.to_string(),
                    output_format: agent_config.output_format.clone(),
                    config: config.clone(),
                    validate_mode: false,
                };
                toolbox.add_tool(sql_tool.name.to_string(), tool.into());
            }
            ToolConfig::ValidateSQL(sql_tool) => {
                let warehouse_config = config
                    .find_warehouse(&sql_tool.warehouse)
                    .unwrap_or_else(|_| panic!("Warehouse {} not found", &sql_tool.warehouse));
                let tool: ExecuteSQLTool = ExecuteSQLTool {
                    warehouse_config: warehouse_config.clone(),
                    tool_name: sql_tool.name.to_string(),
                    tool_description: sql_tool.description.to_string(),
                    output_format: agent_config.output_format.clone(),
                    config: config.clone(),
                    validate_mode: true,
                };
                toolbox.add_tool(sql_tool.name.to_string(), tool.into());
            }
            ToolConfig::Retrieval(retrieval) => {
                let tool = RetrieveTool::new(agent_name, retrieval, config);
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
