use dirs::home_dir;
use serde::Deserialize;
use std::fs;
use std::error::Error;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub defaults: Defaults,
    pub models: Vec<Model>,
    pub warehouses: Vec<Warehouse>,
}

#[derive(Debug, Deserialize)]
pub struct Defaults {
    pub agent: String,
    pub project_path: String,
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
    let home_dir = home_dir().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Home directory not found"))?;
    let config_path = home_dir.join(".config").join("onyx").join("config.yml");
    let config_content = fs::read_to_string(config_path)?;
    let config: Config = serde_yaml::from_str(&config_content)?;
    Ok(config)
}
