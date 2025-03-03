pub mod agent;
pub mod anonymizer;
pub mod retrieval;
pub mod toolbox;
pub mod tools;
pub mod utils;

use std::{path::Path, sync::Arc};

use crate::{
    config::{
        load_config,
        model::{
            AgentConfig, AnonymizerConfig, FileFormat, FlashTextSourceType, Model, OutputFormat,
            ToolConfig,
        },
        ConfigManager,
    },
    connector::Connector,
    errors::OnyxError,
    execute::agent::ToolCall,
    union_tools,
};
use agent::OpenAIAgent;
use anonymizer::{base::Anonymizer, flash_text::FlashTextAnonymizer};
use async_trait::async_trait;
use retrieval::get_vector_store;
use schemars::JsonSchema;
use serde::Deserialize;
use toolbox::ToolBox;
use tools::{ExecuteSQLParams, ExecuteSQLTool, RetrieveParams, RetrieveTool, Tool};

pub async fn setup_agent<P: AsRef<Path>>(
    agent_file: P,
    file_format: &FileFormat,
    config: Arc<ConfigManager>,
) -> Result<(OpenAIAgent, AgentConfig), OnyxError> {
    let agent_config = config.resolve_agent(agent_file).await?;
    let agent = from_config(&config, &agent_config, file_format)
        .await
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

async fn from_config(
    config: &ConfigManager,
    agent_config: &AgentConfig,
    file_format: &FileFormat,
) -> Result<OpenAIAgent, OnyxError> {
    let model = config.resolve_model(&agent_config.model)?;
    let anonymizer: Option<Box<dyn Anonymizer + Send + Sync>> = match &agent_config.anonymize {
        None => None,
        Some(AnonymizerConfig::FlashText {
            source,
            pluralize,
            case_sensitive,
        }) => {
            let mut anonymizer = FlashTextAnonymizer::new(pluralize, case_sensitive);
            let path = match &source {
                FlashTextSourceType::Keywords { keywords_file, .. } => keywords_file,
                FlashTextSourceType::Mapping { mapping_file, .. } => mapping_file,
            };
            let resolved_path = config.resolve_file(path).await?;
            anonymizer.add_keywords_file(source, &resolved_path)?;
            Some(Box::new(anonymizer))
        }
    };
    let toolbox = Arc::new(tools_from_config(config, agent_config).await?);
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

async fn tools_from_config(
    config: &ConfigManager,
    agent_config: &AgentConfig,
) -> Result<ToolBox<MultiTool>, OnyxError> {
    let mut toolbox = ToolBox::new();
    for tool_config in agent_config.tools.iter() {
        match tool_config {
            ToolConfig::ExecuteSQL(sql_tool) => {
                let connector = Connector::from_database(&sql_tool.database, config).await?;
                let tool: ExecuteSQLTool = ExecuteSQLTool {
                    tool_name: sql_tool.name.to_string(),
                    tool_description: sql_tool.description.to_string(),
                    connector,
                    output_format: agent_config.output_format.clone(),
                    validate_mode: false,
                };
                toolbox.add_tool(sql_tool.name.to_string(), tool.into());
            }
            ToolConfig::ValidateSQL(sql_tool) => {
                let connector = Connector::from_database(&sql_tool.database, config).await?;
                let tool: ExecuteSQLTool = ExecuteSQLTool {
                    tool_name: sql_tool.name.to_string(),
                    tool_description: sql_tool.description.to_string(),
                    connector,
                    output_format: agent_config.output_format.clone(),
                    validate_mode: true,
                };
                toolbox.add_tool(sql_tool.name.to_string(), tool.into());
            }
            ToolConfig::Retrieval(retrieval) => {
                let db_path = config
                    .resolve_file(format!(".db-{}-{}", agent_config.name, retrieval.name))
                    .await?;
                let vector_db = get_vector_store(retrieval, &db_path)?;
                let tool = RetrieveTool::new(retrieval, vector_db);
                toolbox.add_tool(retrieval.name.to_string(), tool.into());
            }
        };
    }
    Ok(toolbox)
}

union_tools!(
    MultiTool,
    MultiToolInput,
    ExecuteSQLTool,
    ExecuteSQLParams,
    RetrieveTool,
    RetrieveParams
);
