use serde::Deserialize;
use std::fs;
use std::error::Error;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub warehouses: Vec<Warehouse>,
    pub models: Vec<Model>,
}

#[derive(Deserialize, Debug)]
pub struct Warehouse {
    pub name: String,
    pub r#type: String,
    pub key_path: String,
}

#[derive(Deserialize, Debug)]
pub struct Model {
    pub name: String,
    pub vendor: String,
    pub key_var: String,
    pub model_ref: String,
}

pub fn parse_config() -> Result<Config, Box<dyn Error>> {
    let config_content = fs::read_to_string("config.yml")?;
    let config: Config = serde_yaml::from_str(&config_content)?;
    Ok(config)
}
