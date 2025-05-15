use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::errors::OxyError;

use super::{
    model::{AgentConfig, AppConfig, Config, Database, Model, Workflow},
    storage::{ConfigSource, ConfigStorage},
};

#[derive(Debug, Clone)]
pub struct ConfigManager {
    storage: Arc<ConfigSource>,
    config: Arc<Config>,
}

impl ConfigManager {
    pub(super) fn new(storage: ConfigSource, config: Config) -> Self {
        Self {
            storage: Arc::new(storage),
            config: Arc::new(config),
        }
    }

    pub fn resolve_model(&self, model_name: &str) -> Result<&Model, OxyError> {
        let model = self
            .config
            .models
            .iter()
            .find(|m| match m {
                Model::OpenAI { name, .. } => name,
                Model::Ollama { name, .. } => name,
                Model::Google { name, .. } => name,
                Model::Anthropic { name, .. } => name,
            } == model_name)
            .ok_or_else(|| {
                OxyError::ConfigurationError(format!("Model '{}' not found in config", model_name))
            })?;
        Ok(model)
    }

    pub fn default_model(&self) -> Option<&String> {
        self.config.models.first().map(|m| match m {
            Model::OpenAI { name, .. } => name,
            Model::Ollama { name, .. } => name,
            Model::Google { name, .. } => name,
            Model::Anthropic { name, .. } => name,
        })
    }

    pub fn resolve_database(&self, database_name: &str) -> Result<&Database, OxyError> {
        let database = self
            .config
            .databases
            .iter()
            .find(|w| w.name == database_name)
            .ok_or_else(|| {
                OxyError::ConfigurationError(format!(
                    "Database '{}' not found in config",
                    database_name
                ))
            })?;
        Ok(database)
    }

    pub fn default_database_ref(&self) -> Option<&String> {
        self.config.defaults.as_ref().map(|d| d.database.as_ref())?
    }

    pub async fn resolve_file<P: AsRef<Path>>(&self, file_ref: P) -> Result<String, OxyError> {
        self.storage.fs_link(file_ref).await
    }

    pub async fn resolve_glob(&self, paths: &Vec<String>) -> Result<Vec<String>, OxyError> {
        let mut expanded_paths = Vec::new();
        for path in paths {
            expanded_paths.extend(self.storage.glob(path).await?);
        }
        Ok(expanded_paths)
    }

    pub async fn resolve_workflow<P: AsRef<Path>>(
        &self,
        workflow_name: P,
    ) -> Result<Workflow, OxyError> {
        self.storage.load_workflow_config(workflow_name).await
    }

    pub async fn resolve_agent<P: AsRef<Path>>(
        &self,
        agent_name: P,
    ) -> Result<AgentConfig, OxyError> {
        self.storage.load_agent_config(agent_name).await
    }

    pub fn list_databases(&self) -> Result<&[Database], OxyError> {
        Ok(self.config.databases.as_slice())
    }

    pub async fn list_agents(&self) -> Result<Vec<PathBuf>, OxyError> {
        let agents = self.storage.list_agents().await?;
        tracing::debug!("Agents: {:?}", agents);
        tracing::debug!("Builder: {:?}", self.config.builder_agent);
        if let Some(ref builder_agent) = self.config.builder_agent {
            // hide the builder agent from the list
            let builder_agent_full_path =
                self.storage.fs_link(builder_agent).await.map_err(|_| {
                    OxyError::ConfigurationError("Failed to resolve agent path".to_string())
                })?;
            Ok(agents
                .iter()
                .filter(|agent| agent.display().to_string() != builder_agent_full_path)
                .cloned()
                .collect())
        } else {
            Ok(agents)
        }
    }

    pub async fn list_apps(&self) -> Result<Vec<PathBuf>, OxyError> {
        self.storage.list_apps().await
    }
    pub async fn list_workflows(&self) -> Result<Vec<PathBuf>, OxyError> {
        self.storage.list_workflows().await
    }

    pub async fn resolve_app<P: AsRef<Path>>(&self, app_path: P) -> Result<AppConfig, OxyError> {
        self.storage.load_app_config(app_path).await
    }

    pub async fn get_build_agent(&self) -> Result<AgentConfig, OxyError> {
        if let Some(ref agent) = self.config.builder_agent {
            return self.resolve_agent(agent).await;
        } else {
            Err(OxyError::ConfigurationError(
                "No builder agent specified in config".to_string(),
            ))
        }
    }

    pub async fn get_builder_agent_path(&self) -> Result<PathBuf, OxyError> {
        if let Some(ref agent) = self.config.builder_agent {
            Ok(agent.to_owned())
        } else {
            Err(OxyError::ConfigurationError(
                "No builder agent specified in config".to_string(),
            ))
        }
    }
}
