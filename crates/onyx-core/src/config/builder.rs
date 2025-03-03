use std::path::Path;

use crate::errors::OnyxError;

use super::{
    manager::ConfigManager,
    storage::{ConfigSource, ConfigStorage},
};

pub struct ConfigBuilder {
    storage: Option<ConfigSource>,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self { storage: None }
    }

    pub fn with_project_path<P: AsRef<Path>>(mut self, project_path: P) -> Result<Self, OnyxError> {
        self.storage = Some(ConfigSource::local(project_path)?);
        Ok(self)
    }

    pub async fn build(self) -> Result<ConfigManager, OnyxError> {
        let storage = self.storage.ok_or(OnyxError::RuntimeError(
            "Config source is required".to_string(),
        ))?;
        let config = storage.load_config().await?;
        Ok(ConfigManager::new(storage, config))
    }
}
