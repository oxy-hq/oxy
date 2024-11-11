pub mod agent;
pub mod retrieval;
pub mod toolbox;
pub mod tools;

use crate::{
    connector::Connector,
    union_tools,
    yaml_parsers::{
        agent_parser::{AgentConfig, ToolConfig},
        config_parser::{get_config_path, parse_config, Config, Model},
    },
};
use agent::{LLMAgent, OpenAIAgent};
use async_trait::async_trait;
use minijinja::{context, render, Value};
use schemars::JsonSchema;
use serde::Deserialize;
use std::{fs, path::PathBuf};
use toolbox::ToolBox;
use tools::{ExecuteSQLParams, ExecuteSQLTool, RetrieveParams, RetrieveTool, Tool};

pub async fn setup_agent(
    agent_name: Option<&str>,
) -> anyhow::Result<(Box<dyn LLMAgent + Send + Sync>, PathBuf)> {
    let config_path = get_config_path();
    let config = parse_config(&config_path)?;
    let agent_name = agent_name.unwrap_or(config.defaults.agent.as_ref());
    let agent_config = config.load_config(Some(agent_name))?;
    let agent = from_config(agent_name, &config, &agent_config).await;
    Ok((agent, config_path))
}

pub async fn from_config(
    agent_name: &str,
    config: &Config,
    agent_config: &AgentConfig,
) -> Box<dyn LLMAgent + Send + Sync> {
    let model = config.find_model(&agent_config.model).unwrap();
    let mut tools = ToolBox::<MultiTool>::new();

    let ctx = fill_tools(&mut tools, agent_name, agent_config, config).await;

    let system_instructions = render!(&agent_config.system_instructions, ctx);

    match model {
        Model::OpenAI {
            name: _,
            model_ref,
            key_var,
        } => {
            let api_key = std::env::var(&key_var).unwrap_or_else(|_| {
                panic!("OpenAI key not found in environment variable {}", key_var)
            });
            Box::new(OpenAIAgent::new(
                model_ref,
                None,
                api_key,
                tools,
                system_instructions,
                agent_config.output_type.clone(),
            ))
        }
        Model::Ollama {
            name: _,
            model_ref,
            api_key,
            api_url,
        } => Box::new(OpenAIAgent::new(
            model_ref,
            Some(api_url),
            api_key,
            tools,
            system_instructions,
            agent_config.output_type.clone(),
        )),
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
                let queries = load_queries(config.defaults.project_path.clone(), data);
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

fn load_queries(project_path: PathBuf, paths: &Vec<String>) -> Vec<String> {
    let mut queries = vec![];

    for path in paths {
        queries.extend(load_queries_for_scope(&project_path, path));
    }

    queries
}

fn load_queries_for_scope(project_path: &PathBuf, path: &str) -> Vec<String> {
    let query_path = &project_path.join("data").join(path);
    log::debug!("Query path: {}; scope: {}", query_path.display(), path);

    let mut queries = vec![];
    if let Ok(entries) = fs::read_dir(query_path) {
        for entry in entries.flatten() {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                queries.push(content);
            }
        }
    }
    queries
}
