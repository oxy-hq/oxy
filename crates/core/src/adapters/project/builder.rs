use std::path::Path;

use crate::{
    adapters::{project::manager::ProjectManager, runs::RunsManager, secrets::SecretsManager},
    config::{ConfigBuilder, ConfigManager},
    errors::OxyError,
};

pub struct ProjectBuilder {
    config_manager: Option<ConfigManager>,
    secrets_manager: Option<SecretsManager>,
    runs_manager: Option<RunsManager>,
}

impl Default for ProjectBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectBuilder {
    pub fn new() -> Self {
        Self {
            config_manager: None,
            secrets_manager: None,
            runs_manager: None,
        }
    }

    pub async fn with_project_path<P: AsRef<Path>>(
        mut self,
        project_path: P,
    ) -> Result<Self, OxyError> {
        self.config_manager = Some(
            ConfigBuilder::new()
                .with_project_path(project_path)?
                .build()
                .await?,
        );
        Ok(self)
    }

    pub async fn with_project_path_and_fallback_config<P: AsRef<Path>>(
        mut self,
        project_path: P,
    ) -> Result<Self, OxyError> {
        self.config_manager = Some(
            ConfigBuilder::new()
                .with_project_path(project_path)?
                .build_with_fallback_config()
                .await?,
        );
        Ok(self)
    }

    pub fn with_secrets_manager(mut self, secret_manager: SecretsManager) -> Self {
        self.secrets_manager = Some(secret_manager);
        self
    }

    pub fn with_runs_manager(mut self, runs_manager: RunsManager) -> Self {
        self.runs_manager = Some(runs_manager);
        self
    }

    pub async fn build(self) -> Result<ProjectManager, OxyError> {
        let config_manager = self.config_manager.ok_or(OxyError::RuntimeError(
            "Config source is required".to_string(),
        ))?;

        let secret_manager = self
            .secrets_manager
            .or(Some(SecretsManager::from_environment().unwrap()))
            .unwrap();
        Ok(ProjectManager::new(
            config_manager,
            secret_manager,
            self.runs_manager,
        ))
    }
}
