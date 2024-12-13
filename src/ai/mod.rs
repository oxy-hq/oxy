pub mod agent;
pub mod anonymizer;
pub mod retrieval;
pub mod toolbox;
pub mod tools;
pub mod utils;

use crate::{
    config::{
        load_config,
        model::{
            AgentConfig, AnonymizerConfig, Config, FileFormat, Model, ProjectPath, ToolConfig,
        },
    },
    connector::Connector,
    union_tools,
};
use agent::{LLMAgent, OpenAIAgent};
use anonymizer::{base::Anonymizer, flash_text::FlashTextAnonymizer};
use async_trait::async_trait;
use minijinja::{context, render, Value};
use schemars::JsonSchema;
use serde::Deserialize;
use std::{fs, path::PathBuf};
use toolbox::ToolBox;
use tools::{ExecuteSQLParams, ExecuteSQLTool, RetrieveParams, RetrieveTool, Tool};

pub async fn setup_agent(
    agent_file: Option<&PathBuf>,
    file_format: &FileFormat,
) -> anyhow::Result<(Box<dyn LLMAgent + Send + Sync>)> {
    let config = load_config()?;

    let (agent_config, agent_name) = config.load_agent_config(agent_file)?;
    let agent = from_config(&agent_name, &config, &agent_config, file_format).await?;
    Ok(agent)
}

pub async fn from_config(
    agent_name: &str,
    config: &Config,
    agent_config: &AgentConfig,
    file_format: &FileFormat,
) -> anyhow::Result<Box<dyn LLMAgent + Send + Sync>> {
    let model = config.find_model(&agent_config.model).unwrap();
    let mut tools = ToolBox::<MultiTool>::new();
    let ctx = fill_tools(&mut tools, agent_name, agent_config, config).await;
    let system_instructions = render!(&agent_config.system_instructions, ctx);
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

    match model {
        Model::OpenAI {
            name: _,
            model_ref,
            key_var,
        } => {
            let api_key = std::env::var(&key_var).unwrap_or_else(|_| {
                panic!("OpenAI key not found in environment variable {}", key_var)
            });
            Ok(Box::new(OpenAIAgent::new(
                model_ref,
                None,
                api_key,
                tools,
                system_instructions,
                agent_config.output_format.clone(),
                anonymizer,
                file_format.clone(),
            )))
        }
        Model::Ollama {
            name: _,
            model_ref,
            api_key,
            api_url,
        } => Ok(Box::new(OpenAIAgent::new(
            model_ref,
            Some(api_url),
            api_key,
            tools,
            system_instructions,
            agent_config.output_format.clone(),
            anonymizer,
            file_format.clone(),
        ))),
    }
}

union_tools!(
    MultiTool,
    MultiToolInput,
    ExecuteSQLTool,
    ExecuteSQLParams,
    RetrieveTool,
    RetrieveParams
);

async fn fill_tools(
    toolbox: &mut ToolBox<MultiTool>,
    agent_name: &str,
    agent_config: &AgentConfig,
    config: &Config,
) -> Value {
    let mut tool_ctx = context! {};

    for tool_config in agent_config.tools.as_ref().unwrap() {
        match tool_config {
            ToolConfig::ExecuteSQL {
                name,
                description,
                warehouse,
            } => {
                let warehouse_config = config
                    .find_warehouse(warehouse)
                    .unwrap_or_else(|_| panic!("Warehouse {} not found", &warehouse));
                let warehouse_info = Connector::new(&warehouse_config)
                    .load_warehouse_info()
                    .await;
                tool_ctx = context! {
                    warehouse => warehouse_info,
                    ..tool_ctx,
                };
                let tool = ExecuteSQLTool {
                    config: warehouse_config.clone(),
                    tool_description: description.to_string(),
                    output_format: agent_config.output_format.clone(),
                };
                toolbox.add_tool(name.to_string(), tool.into());
            }
            ToolConfig::Retrieval {
                name,
                description,
                data,
            } => {
                // let retrieval = config
                //    .find_retrieval(agent_config.retrieval.as_ref().unwrap())
                //     .unwrap();
                let queries = load_queries(data);
                tool_ctx = context! {
                    queries => queries,
                    ..tool_ctx,
                };

                // let tool = RetrieveTool::new(agent_name, name, retrieval, description);
                // toolbox.add_tool(name.to_string(), tool.into());
            }
        };
    }
    tool_ctx
}

fn load_queries(paths: &Vec<String>) -> Vec<String> {
    let mut queries = vec![];

    for path in paths {
        log::debug!("Loading queries for path: {}", path);
        queries.extend(load_queries_for_scope(path));
        log::debug!("Loaded queries");
    }

    queries
}

fn load_queries_for_scope(path: &str) -> Vec<String> {
    let query_path = &ProjectPath::get_path("data").join(path);
    log::debug!("Query path: {}; scope: {}", query_path.display(), path);

    let mut queries = vec![];
    if let Ok(entries) = fs::read_dir(query_path) {
        log::debug!("Reading queries from path: {}", query_path.display());
        for entry in entries.flatten() {
            log::debug!("Reading query: {}", entry.path().display());
            if let Ok(content) = fs::read_to_string(entry.path()) {
                queries.push(content);
            }
        }
    }
    queries
}
