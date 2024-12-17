pub mod agent;
pub mod anonymizer;
pub mod retrieval;
pub mod toolbox;
pub mod tools;
pub mod utils;

use glob::glob;

use crate::{
    config::{
        load_config,
        model::{
            AgentConfig, AnonymizerConfig, Config, Context, FileFormat, Model, ProjectPath,
            ToolConfig,
        },
    },
    connector::Connector,
    union_tools, StyledText,
};
use agent::{LLMAgent, OpenAIAgent};
use anonymizer::{base::Anonymizer, flash_text::FlashTextAnonymizer};
use async_trait::async_trait;
use minijinja::{context, render, Value};
use schemars::JsonSchema;
use serde::Deserialize;
use std::{collections::HashMap, fs, path::PathBuf};
use toolbox::ToolBox;
use tools::{ExecuteSQLParams, ExecuteSQLTool, RetrieveParams, RetrieveTool, Tool};

pub async fn setup_agent(
    agent_file: Option<&PathBuf>,
    file_format: &FileFormat,
) -> anyhow::Result<Box<dyn LLMAgent + Send + Sync>> {
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
    let (tools, context, toolbox) = prepare_contexts(agent_config, config).await;
    let system_instructions =
        render!(&agent_config.system_instructions, tools => tools, context => context);
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
                toolbox,
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
            toolbox,
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

fn list_files_from_pattern(pattern: String) -> Vec<PathBuf> {
    let paths_rs = glob(pattern.as_str());
    let mut paths: Vec<PathBuf> = vec![];
    match paths_rs {
        Ok(paths_rs) => {
            for path in paths_rs {
                match path {
                    Ok(p) => {
                        paths.push(p);
                    }
                    Err(e) => {
                        println!("{} {:?}", "Error loading files".warning(), e);
                    }
                }
            }
        }
        Err(e) => {
            println!("{} {:?}", "Error loading files".warning(), e);
        }
    }

    paths
}

fn create_jinja_context(ctxs: &Vec<Context>) -> HashMap<String, Vec<String>> {
    let mut ctx_map: HashMap<String, Vec<String>> = HashMap::new();
    for c in ctxs {
        let mut paths: Vec<PathBuf> = vec![];
        for src in c.src.clone() {
            paths.extend(list_files_from_pattern(src));
        }
        let mut contents = vec![];
        for path in paths {
            match fs::read_to_string(path) {
                Ok(content) => {
                    contents.push(content);
                }
                Err(e) => {
                    println!("{} {:?}", "Error reading context".warning(), e);
                }
            }
        }

        ctx_map.insert(c.name.clone(), contents);
    }

    ctx_map
}

async fn prepare_contexts(
    agent_config: &AgentConfig,
    config: &Config,
) -> (Value, Value, ToolBox<MultiTool>) {
    let mut toolbox = ToolBox::<MultiTool>::new();
    let mut tool_ctx = context! {};
    let mut oth_ctx = context! {};
    if agent_config.context.is_some() {
        let ctxs: &Vec<Context> = agent_config.context.as_ref().unwrap();
        oth_ctx = Value::from(create_jinja_context(ctxs));
    }

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
                oth_ctx = context! {
                    warehouse => warehouse_info,
                    ..oth_ctx,
                };
                let tool: ExecuteSQLTool = ExecuteSQLTool {
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

    (tool_ctx, oth_ctx, toolbox)
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
    let query_path = &ProjectPath::get_path(path);
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
