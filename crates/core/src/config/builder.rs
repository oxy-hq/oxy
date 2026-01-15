use std::path::Path;

use oxy_shared::errors::OxyError;

use super::{
    manager::ConfigManager,
    storage::{ConfigSource, ConfigStorage},
};

pub struct ConfigBuilder {
    storage: Option<ConfigSource>,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self { storage: None }
    }

    pub fn with_project_path<P: AsRef<Path>>(mut self, project_path: P) -> Result<Self, OxyError> {
        self.storage = Some(ConfigSource::local(project_path)?);
        Ok(self)
    }

    pub async fn build(self) -> Result<ConfigManager, OxyError> {
        let storage = self.storage.ok_or(OxyError::RuntimeError(
            "Config source is required".to_string(),
        ))?;

        let config = storage.load_config().await?;
        Ok(ConfigManager::new(storage, config))
    }

    pub async fn build_with_fallback_config(self) -> Result<ConfigManager, OxyError> {
        let storage = self.storage.ok_or(OxyError::RuntimeError(
            "Config source is required".to_string(),
        ))?;

        let config = storage.load_config_with_fallback().await;
        Ok(ConfigManager::new(storage, config))
    }
}
