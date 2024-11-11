use crate::yaml_parsers::agent_parser::{parse_agent_config, AgentConfig};
use dirs::home_dir;
use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};

use super::workflow_parser::{parse_workflow_config, Workflow};

#[derive(Deserialize, Debug)]
pub struct Config {
    pub defaults: Defaults,
    pub models: Vec<Model>,
    pub warehouses: Vec<Warehouse>,
    pub retrievals: Vec<Retrieval>,
}

// These are settings stored as strings derived from the config.yml file's defaults section
#[derive(Debug, Deserialize)]
pub struct Defaults {
    pub agent: String,
    pub project_path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Warehouse {
    pub name: String,
    pub r#type: String,
    #[serde(default)]
    pub key_path: String,
    pub dataset: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "vendor")]
pub enum Model {
    #[serde(rename = "openai")]
    OpenAI {
        name: String,
        model_ref: String,
        key_var: String,
    },
    #[serde(rename = "ollama")]
    Ollama {
        name: String,
        model_ref: String,
        api_key: String,
        api_url: String,
    },
}

#[derive(Deserialize, Debug, Clone)]
pub struct Retrieval {
    pub name: String,
    pub embed_model: String,
    pub rerank_model: String,
    pub top_k: usize,
    pub factor: usize,
}

pub fn get_config_path() -> PathBuf {
    home_dir()
        .expect("Could not find home directory")
        .join(".config")
        .join("onyx")
        .join("config.yml")
}

pub fn parse_config(config_path: &PathBuf) -> anyhow::Result<Config> {
    let config_str = fs::read_to_string(config_path)?;
    let config: Config = serde_yaml::from_str(&config_str)?;
    Ok(config)
}

#[derive(Debug)]
pub struct ParsedConfig {
    pub agent_config: AgentConfig,
    pub model: Model,
    pub warehouse: Warehouse,
    pub retrieval: Retrieval,
}

impl Config {
    pub fn load_config(&self, agent_name: Option<&str>) -> anyhow::Result<AgentConfig> {
        let agent_file = if let Some(name) = agent_name {
            PathBuf::from(&self.defaults.project_path)
                .join("agents")
                .join(format!("{}.yml", name))
        } else {
            PathBuf::from(&self.defaults.project_path)
                .join("agents")
                .join(format!("{}.yml", self.defaults.agent))
        };

        if !agent_file.exists() {
            return Err(anyhow::Error::msg(format!(
                "Agent configuration file not found: {:?}",
                agent_file
            )));
        }

        let agent_config = parse_agent_config(&agent_file.to_string_lossy())?;
        Ok(agent_config)
    }

    pub fn load_workflow(&self, workflow_name: &str) -> anyhow::Result<Workflow> {
        let workflow_file = PathBuf::from(&self.defaults.project_path)
            .join("workflows")
            .join(format!("{}.yml", workflow_name));

        if !workflow_file.exists() {
            return Err(anyhow::Error::msg(format!(
                "Workflow configuration file not found: {:?}",
                workflow_file
            )));
        }

        let workflow_config =
            parse_workflow_config(workflow_name, &workflow_file.to_string_lossy())?;
        Ok(workflow_config)
    }

    pub fn find_model(&self, model_name: &str) -> anyhow::Result<Model> {
        self.models
            .iter()
            .find(|m| {
                match match m {
                    Model::OpenAI { name, .. } => name,
                    Model::Ollama { name, .. } => name,
                } {
                    name => name == model_name,
                }
            })
            .cloned()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "Default model not found").into()
            })
    }

    pub fn find_warehouse(&self, warehouse_name: &str) -> anyhow::Result<Warehouse> {
        self.warehouses
            .iter()
            .find(|w| w.name == warehouse_name)
            .cloned()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "Default warehouse not found").into()
            })
    }

    pub fn find_retrieval(&self, retrieval_name: &str) -> anyhow::Result<Retrieval> {
        self.retrievals
            .iter()
            .find(|m| m.name == retrieval_name)
            .cloned()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "Default retrieval not found").into()
            })
    }
}
