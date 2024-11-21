use std::{path::PathBuf, rc::Rc};
pub mod model;
mod parser;
mod validate;
use garde::Validate;

use anyhow;
use model::{AgentConfig, Config, Model, Retrieval, Warehouse, Workflow};

use dirs::home_dir;
use parser::{parse_agent_config, parse_workflow_config};
use serde::Deserialize;
use std::{fs, io};
use validate::ValidationContext;

// These are settings stored as strings derived from the config.yml file's defaults section
#[derive(Debug, Deserialize)]
pub struct Defaults {
    pub agent: String,
    pub project_path: PathBuf,
}

impl Defaults {
    pub fn expand_project_path(&mut self) {
        if let Some(str_path) = self.project_path.to_str() {
            if str_path.starts_with("~") {
                if let Some(home) = home_dir() {
                    self.project_path = home.join(str_path.trim_start_matches("~"));
                }
            }
        }
    }
}

pub fn get_config_path() -> PathBuf {
    home_dir()
        .expect("Could not find home directory")
        .join(".config")
        .join("onyx")
        .join("config.yml")
}

#[derive(Debug)]
pub struct ParsedConfig {
    pub agent_config: AgentConfig,
    pub model: Model,
    pub warehouse: Warehouse,
    pub retrieval: Retrieval,
}

impl Config {
    pub fn get_agents_dir(&self) -> PathBuf {
        return PathBuf::from(&self.defaults.project_path).join("agents");
    }

    pub fn get_sql_dir(&self) -> PathBuf {
        return PathBuf::from(&self.defaults.project_path).join("data");
    }

    pub fn load_config(&self, agent_name: Option<&str>) -> anyhow::Result<AgentConfig> {
        let agent_file = if let Some(name) = agent_name {
            self.get_agents_dir().join(format!("{}.yml", name))
        } else {
            self.get_agents_dir()
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

    pub fn list_workflows(&self) -> anyhow::Result<Vec<String>> {
        let workflow_dir = PathBuf::from(&self.defaults.project_path).join("workflows");

        let mut workflows = vec![];
        for entry in fs::read_dir(workflow_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "yml" {
                        let file_stem = path.file_stem().unwrap().to_str().unwrap();
                        workflows.push(file_stem.to_string());
                    }
                }
            }
        }

        Ok(workflows)
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

pub fn load_config() -> anyhow::Result<Config> {
    let config_path = get_config_path();
    let config = parse_config(&config_path)?;

    Ok(config)
}

pub fn parse_config(config_path: &PathBuf) -> anyhow::Result<Config> {
    let config_str = fs::read_to_string(&config_path)?;
    let result = serde_yaml::from_str::<Config>(&config_str);
    match result {
        Ok(config) => {
            let rc = Rc::new(config);
            let context = ValidationContext {
                config: Rc::clone(&rc),
            };
            let mut validation_result = Rc::clone(&rc).validate_with(&context);
            if validation_result.is_ok() {
                let workflows = Rc::clone(&rc).list_workflows()?;
                for workflow in workflows {
                    let workflow_config = Rc::clone(&rc).load_workflow(&workflow)?;
                    let workflow_context = ValidationContext {
                        config: Rc::clone(&rc),
                    };
                    validation_result = workflow_config.validate_with(&workflow_context);
                }
            }
            drop(context);
            match validation_result {
                Ok(_) => match Rc::try_unwrap(rc) {
                    Ok(config) => return Ok(config),
                    Err(_) => return Err(anyhow::anyhow!("Failed to unwrap Rc")),
                },
                Err(e) => {
                    return Err(anyhow::anyhow!("Invalid configuration: \n{}", e));
                }
            }
        }
        Err(e) => {
            let mut rawError = e.to_string();
            rawError = rawError.replace("usize", "unsigned integer");
            return Err(anyhow::anyhow!(
                "Failed to parse config file:\n{}",
                rawError
            ));
        }
    }
}
