use std::path::{Path, PathBuf};

use crate::errors::OnyxError;

use super::{
    model::{AgentConfig, Config, Database, Model, Workflow},
    storage::{ConfigSource, ConfigStorage},
};

#[derive(Debug)]
pub struct ConfigManager {
    storage: ConfigSource,
    config: Config,
}

impl ConfigManager {
    pub(super) fn new(storage: ConfigSource, config: Config) -> Self {
        Self { storage, config }
    }

    pub fn resolve_model(&self, model_name: &str) -> Result<&Model, OnyxError> {
        let model = self
            .config
            .models
            .iter()
            .find(|m| {
                match match m {
                    Model::OpenAI { name, .. } => name,
                    Model::Ollama { name, .. } => name,
                } {
                    name => name == model_name,
                }
            })
            .ok_or_else(|| {
                OnyxError::ConfigurationError(format!("Model '{}' not found in config", model_name))
            })?;
        Ok(model)
    }

    pub fn default_model(&self) -> Option<&String> {
        self.config.models.first().map(|m| match m {
            Model::OpenAI { name, .. } => name,
            Model::Ollama { name, .. } => name,
        })
    }

    pub fn resolve_database(&self, database_name: &str) -> Result<&Database, OnyxError> {
        let database = self
            .config
            .databases
            .iter()
            .find(|w| w.name == database_name)
            .ok_or_else(|| {
                OnyxError::ConfigurationError(format!(
                    "Database '{}' not found in config",
                    database_name
                ))
            })?;
        Ok(database)
    }

    pub fn default_database_ref(&self) -> Option<&String> {
        self.config.defaults.as_ref().map(|d| d.database.as_ref())?
    }

    pub async fn resolve_file<P: AsRef<Path>>(&self, file_ref: P) -> Result<String, OnyxError> {
        self.storage.fs_link(file_ref).await
    }

    pub async fn resolve_glob(&self, paths: &Vec<String>) -> Result<Vec<String>, OnyxError> {
        let mut expanded_paths = Vec::new();
        for path in paths {
            expanded_paths.extend(self.storage.glob(path).await?);
        }
        Ok(expanded_paths)
    }

    pub async fn resolve_workflow<P: AsRef<Path>>(
        &self,
        workflow_name: P,
    ) -> Result<Workflow, OnyxError> {
        self.storage.load_workflow_config(workflow_name).await
    }

    pub async fn resolve_agent<P: AsRef<Path>>(
        &self,
        agent_name: P,
    ) -> Result<AgentConfig, OnyxError> {
        self.storage.load_agent_config(agent_name).await
    }

    pub async fn list_agents(&self) -> Result<Vec<PathBuf>, OnyxError> {
        self.storage.list_agents().await
    }
}
