use dirs::home_dir;
use serde::Deserialize;
use std::{fs, io, path::PathBuf};
use std::error::Error;
use crate::yaml_parsers::agent_parser::{Agent, parse_agent_config};

#[derive(Deserialize, Debug)]
pub struct Config {
    pub defaults: Defaults,
    pub models: Vec<Model>,
    pub warehouses: Vec<Warehouse>,
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
}

#[derive(Deserialize, Debug, Clone)]
pub struct Model {
    pub name: String,
    pub vendor: String,
    pub key_var: String,
    pub model_ref: String,
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

pub struct ParsedConfig {
    pub agent: Agent,
    pub model: Model,
    pub warehouse: Warehouse
}

impl Config {

    pub fn load_defaults(&self) -> Result<ParsedConfig, Box<dyn Error>> {

        let default_agent_file = PathBuf::from(&self.defaults.project_path)
            .join("agents")
            .join(format!("{}.yml", self.defaults.agent));

        let agent = parse_agent_config(&default_agent_file.to_string_lossy())?;
        let model = self.load_model(&agent.model)?;
        let warehouse = self.load_warehouse(&agent.warehouse)?;

        Ok(ParsedConfig {
            agent,
            model,
            warehouse,
        })
    }

    fn load_model(&self, model_name: &str) -> Result<Model, Box<dyn Error>> {
        self.models.iter()
            .find(|m| m.name == model_name)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Default model not found").into())
    }

    fn load_warehouse(&self, warehouse_name: &str) -> Result<Warehouse, Box<dyn Error>> {
        self.warehouses.iter()
            .find(|w| w.name == warehouse_name)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Default warehouse not found").into())
    }
}
