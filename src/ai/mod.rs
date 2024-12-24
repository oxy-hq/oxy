pub mod agent;
pub mod anonymizer;
pub mod retrieval;
pub mod toolbox;
pub mod tools;
pub mod utils;

use glob::glob;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::PathBuf,
};

use crate::{
    config::{
        load_config,
        model::{
            AgentConfig, AgentContext, AgentContextType, AnonymizerConfig, Config, FileFormat,
            Model, ProjectPath, SemanticModelContext, ToolConfig,
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
    let (tools, context, toolbox) = prepare_contexts(agent_name, agent_config, config).await;
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
            api_url,
            azure_deployment_id,
            azure_api_version,
        } => {
            let api_key = std::env::var(&key_var).unwrap_or_else(|_| {
                panic!("OpenAI key not found in environment variable {}", key_var)
            });
            Ok(Box::new(OpenAIAgent::new(
                model_ref,
                api_url,
                api_key,
                azure_deployment_id,
                azure_api_version,
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
            None,
            None,
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

fn list_files_from_pattern(pattern: &String) -> Vec<PathBuf> {
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

async fn create_jinja_context(ctxs: &Vec<AgentContext>, config: &Config) -> anyhow::Result<Value> {
    let mut ctx = context! {};
    for c in ctxs {
        match &c.context_type {
            AgentContextType::SemanticModel(semantic_model_context) => {
                let semantic_model_context =
                    fill_semantic_model_context(&c.name, semantic_model_context, config).await?;
                ctx = context! {
                    ..semantic_model_context,
                    ..ctx,
                }
            }
            AgentContextType::File(file_context) => {
                let mut paths: Vec<PathBuf> = vec![];
                for src in &file_context.src.clone() {
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
                let mut ctx_map: HashMap<String, Vec<String>> = HashMap::new();
                ctx_map.insert(c.name.clone(), contents);
                ctx = context! {
                    ..Value::from(ctx_map),
                    ..ctx,
                }
            }
        }
    }

    Ok(ctx)
}

async fn prepare_contexts(
    agent_name: &str,
    agent_config: &AgentConfig,
    config: &Config,
) -> (Value, Value, ToolBox<MultiTool>) {
    let mut toolbox = ToolBox::<MultiTool>::new();
    let mut tool_ctx = context! {};
    let mut oth_ctx = context! {};
    if agent_config.context.is_some() {
        let ctxs: &Vec<AgentContext> = agent_config.context.as_ref().unwrap();
        oth_ctx = create_jinja_context(ctxs, config).await.unwrap();
    }

    for tool_config in agent_config.tools.iter() {
        match tool_config {
            ToolConfig::ExecuteSQL(execute_sql) => {
                let warehouse_config = config
                    .find_warehouse(&execute_sql.warehouse)
                    .unwrap_or_else(|_| panic!("Warehouse {} not found", &execute_sql.warehouse));
                let warehouse_info = Connector::new(&warehouse_config)
                    .load_warehouse_info()
                    .await;
                oth_ctx = context! {
                    warehouse => warehouse_info,
                    ..oth_ctx,
                };
                let tool: ExecuteSQLTool = ExecuteSQLTool {
                    config: warehouse_config.clone(),
                    tool_description: execute_sql.description.to_string(),
                    output_format: agent_config.output_format.clone(),
                };
                toolbox.add_tool(execute_sql.name.to_string(), tool.into());
            }
            ToolConfig::Retrieval(retrieval) => {
                let tool = RetrieveTool::new(agent_name, retrieval);
                toolbox.add_tool(retrieval.name.to_string(), tool.into());
            }
        };
    }

    (tool_ctx, oth_ctx, toolbox)
}

async fn fill_semantic_model_context(
    context_name: &String,
    semantic_model_context: &SemanticModelContext,
    config: &Config,
) -> anyhow::Result<Value> {
    let path = &ProjectPath::get_path(&semantic_model_context.src);
    let semantic_model = config.load_semantic_model(path)?;
    let warehouse: &String = &semantic_model.warehouse;
    let warehouse_config = config
        .find_warehouse(warehouse)
        .unwrap_or_else(|_| panic!("Warehouse {} not found", warehouse));

    let mut semantic_model_ctx = BTreeMap::new();
    semantic_model_ctx.insert(
        context_name.to_string(),
        context! {
            table => semantic_model.table,
            warehouse => warehouse_config.clone(),
            description => semantic_model.description,
            entities => semantic_model.entities,
            dimensions => semantic_model.dimensions,
            measures => semantic_model.measures,
        },
    );

    Ok(Value::from(semantic_model_ctx))
}
