use crate::yaml_parsers::agent_parser::{parse_agent_config, AgentConfig};
use dirs::home_dir;
use serde::Deserialize;
use std::error::Error;
use std::{fs, io, path::PathBuf};

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

#[derive(Deserialize, Debug, Clone)]
pub struct Warehouse {
    pub name: String,
    pub r#type: String,
    pub key_path: String,
    pub dataset: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Model {
    pub name: String,
    pub vendor: String,
    pub key_var: String,
    pub model_ref: String,
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

pub fn parse_config(config_path: PathBuf) -> Result<Config, Box<dyn Error>> {
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
    pub fn load_config(&self, agent_name: Option<&str>) -> Result<ParsedConfig, Box<dyn Error>> {
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
            return Err(Box::new(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Agent configuration file not found: {:?}", agent_file),
            )));
        }

        let agent_config = parse_agent_config(&agent_file.to_string_lossy())?;
        let model = self.load_model(&agent_config.model)?;
        let warehouse = self.load_warehouse(&agent_config.warehouse)?;
        let retrieval = self.load_retrieval(&agent_config.retrieval)?;

        Ok(ParsedConfig {
            agent_config,
            model,
            warehouse,
            retrieval,
        })
    }

    fn load_model(&self, model_name: &str) -> Result<Model, Box<dyn Error>> {
        self.models
            .iter()
            .find(|m| m.name == model_name)
            .cloned()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "Default model not found").into()
            })
    }

    fn load_warehouse(&self, warehouse_name: &str) -> Result<Warehouse, Box<dyn Error>> {
        self.warehouses
            .iter()
            .find(|w| w.name == warehouse_name)
            .cloned()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "Default warehouse not found").into()
            })
    }

    fn load_retrieval(&self, retrieval_name: &str) -> Result<Retrieval, Box<dyn Error>> {
        self.retrievals
            .iter()
            .find(|m| m.name == retrieval_name)
            .cloned()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "Default retrieval not found").into()
            })
    }
}
