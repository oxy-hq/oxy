use std::path::PathBuf;
pub mod auth;
pub mod model;
mod parser;
pub mod schema_type_converter;
pub mod validate;
use garde::Validate;
mod builder;
pub mod constants;
mod manager;
pub mod oxy;
mod storage;

use anyhow;
use model::{AgentConfig, Config, Database, Model, SemanticModels, Workflow};

use parser::{parse_agent_config, parse_semantic_model_config, parse_workflow_config};
use std::{fs, io};
use validate::{AgentValidationContext, ValidationContext};

use crate::errors::OxyError;

pub use builder::ConfigBuilder;
pub use manager::ConfigManager;

impl Config {
    pub fn validate_config(&self) -> anyhow::Result<()> {
        let context = ValidationContext {
            config: self.clone(),
            metadata: None,
        };
        match self.validate_with(&context) {
            Ok(_) => Ok(()),
            Err(e) => anyhow::bail!(OxyError::ConfigurationError(format!(
                "Invalid configuration: {e}"
            ))),
        }
    }

    pub fn validate_workflow(&self, workflow: &Workflow) -> anyhow::Result<()> {
        let context = ValidationContext {
            config: self.clone(),
            metadata: None,
        };
        match workflow.validate_with(&context) {
            Ok(_) => Ok(()),
            Err(e) => anyhow::bail!(OxyError::ConfigurationError(format!(
                "Invalid workflow: {} \n{}",
                workflow.name, e
            ))),
        }
    }

    pub fn validate_agent(&self, agent: &AgentConfig, path: String) -> anyhow::Result<()> {
        let context = AgentValidationContext {
            config: self.clone(),
            agent_config: agent.clone(),
        };
        match agent.validate_with(&context) {
            Ok(_) => Ok(()),
            Err(e) => anyhow::bail!(OxyError::ConfigurationError(format!(
                "Invalid agent: {path} \n{e}"
            ))),
        }
    }

    pub fn validate_workflows(&self) -> anyhow::Result<()> {
        for workflow_file in self.list_workflows(&self.project_path) {
            let workflow = self.load_workflow(&workflow_file)?;
            self.validate_workflow(&workflow)?;
        }
        Ok(())
    }

    pub fn validate_agents(&self) -> anyhow::Result<()> {
        for agent in self.list_agents(&self.project_path) {
            let agent = self.load_agent_config(Some(&agent))?;
            self.validate_agent(&agent.0, agent.1)?;
        }
        Ok(())
    }

    pub fn load_agent_config(
        &self,
        agent_file: Option<&PathBuf>,
    ) -> Result<(AgentConfig, String), OxyError> {
        let agent_file = agent_file.unwrap();
        if !agent_file.exists() {
            return Err(OxyError::ConfigurationError(format!(
                "Agent configuration file not found: {agent_file:?}"
            )));
        }

        let agent_config = parse_agent_config(&agent_file.to_string_lossy())?;

        let agent_name = agent_file.file_stem().unwrap().to_str().unwrap();
        let agent_name = agent_name.strip_suffix(".agent").unwrap_or(agent_name);

        Ok((agent_config, agent_name.to_owned()))
    }

    fn list_by_sub_extension(&self, dir: &PathBuf, sub_extension: &str) -> Vec<PathBuf> {
        let mut files = Vec::new();

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    files.extend(self.list_by_sub_extension(&path, sub_extension));
                } else if path.is_file()
                    && path.extension().and_then(|s| s.to_str()) == Some("yml")
                    && path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .map(|s| s.ends_with(format!(".{sub_extension}.yml").as_str()))
                        .unwrap_or(false)
                {
                    files.push(path);
                }
            }
        }

        files
    }

    pub fn list_agents(&self, dir: &PathBuf) -> Vec<PathBuf> {
        self.list_by_sub_extension(dir, "agent")
    }

    pub fn list_workflows(&self, dir: &PathBuf) -> Vec<PathBuf> {
        self.list_by_sub_extension(dir, "workflow")
    }

    pub fn list_apps(&self, dir: &PathBuf) -> Vec<PathBuf> {
        self.list_by_sub_extension(dir, "app")
    }

    pub fn load_workflow(&self, workflow_path: &PathBuf) -> Result<Workflow, OxyError> {
        if !workflow_path.exists() {
            return Err(OxyError::ArgumentError(format!(
                "Workflow configuration file not found: {workflow_path:?}"
            )));
        }

        let workflow_name = workflow_path.file_stem().unwrap().to_str().unwrap();
        let workflow_name = workflow_name
            .strip_suffix(".workflow")
            .unwrap_or(workflow_name);

        let workflow_config =
            parse_workflow_config(workflow_name, &workflow_path.to_string_lossy())?;

        Ok(workflow_config)
    }

    pub fn load_semantic_model(
        &self,
        semantic_model_path: &PathBuf,
    ) -> anyhow::Result<SemanticModels> {
        if !semantic_model_path.exists() {
            anyhow::bail!(OxyError::ConfigurationError(format!(
                "Semantic model file not found: {semantic_model_path:?}"
            )));
        }

        let semantic_model = parse_semantic_model_config(&semantic_model_path.to_string_lossy())?;

        Ok(semantic_model)
    }

    pub fn default_model(&self) -> Option<String> {
        self.models.first().map(|m| match m {
            Model::OpenAI { name, .. } => name.clone(),
            Model::Ollama { name, .. } => name.clone(),
            Model::Google { name, .. } => name.clone(),
            Model::Anthropic { name, .. } => name.clone(),
        })
    }

    pub fn find_model(&self, model_name: &str) -> anyhow::Result<Model> {
        self.models
            .iter()
            .find(|m| match m {
                Model::OpenAI { name, .. } => name,
                Model::Ollama { name, .. } => name,
                Model::Google { name, .. } => name,
                Model::Anthropic { name, .. } => name,
            } == model_name)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Model not found").into())
    }

    pub fn find_database(&self, database_name: &str) -> anyhow::Result<Database> {
        self.databases
            .iter()
            .find(|w| w.name == database_name)
            .cloned()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Database {database_name} not found"),
                )
                .into()
            })
    }
}

pub fn parse_config(config_path: &PathBuf, project_path: PathBuf) -> Result<Config, OxyError> {
    let config_str = fs::read_to_string(config_path)
        .map_err(|_e| OxyError::ConfigurationError("Unable to read config file".into()))?;

    let result = serde_yaml::from_str::<Config>(&config_str);
    match result {
        Ok(mut config) => {
            config.project_path = project_path;
            let context = ValidationContext {
                config: config.clone(),
                metadata: None,
            };
            let validation_result = config
                .validate_with(&context)
                .map_err(|e| OxyError::ConfigurationError(e.to_string()));
            match validation_result {
                Ok(_) => Ok(config),
                Err(e) => Err(e),
            }
        }
        Err(e) => {
            let mut raw_error = e.to_string();
            raw_error = raw_error.replace("usize", "unsigned integer");
            Err(OxyError::ConfigurationError(format!(
                "Failed to parse config file:\n{raw_error}"
            )))
        }
    }
}

pub fn resolve_local_project_path() -> Result<PathBuf, OxyError> {
    let mut current_dir = std::env::current_dir().expect("Could not get current directory");

    for _ in 0..10 {
        let config_path = current_dir.join("config.yml");
        if config_path.exists() {
            return Ok(current_dir);
        }

        if !current_dir.pop() {
            break;
        }
    }

    Err(OxyError::RuntimeError(
        "Could not find config.yml".to_string(),
    ))
}
