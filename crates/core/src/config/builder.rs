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

    pub fn with_workspace_path<P: AsRef<Path>>(
        mut self,
        workspace_path: P,
    ) -> Result<Self, OxyError> {
        self.storage = Some(ConfigSource::local(workspace_path)?);
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

    /// Build a `ConfigManager` with a pre-resolved `Config` instead of
    /// re-parsing `config.yml` from disk. The storage layer is still
    /// constructed locally because path-validation, `resolve_file`, and
    /// other operations need a workspace path even when the runtime
    /// `Config` came from Postgres. The caller is expected to set
    /// `config.workspace_path` (which is `#[serde(skip)]` on the
    /// struct, so it won't have been populated by deserialisation).
    pub fn build_with_provided_config(
        self,
        config: crate::config::model::Config,
    ) -> Result<ConfigManager, OxyError> {
        let storage = self.storage.ok_or(OxyError::RuntimeError(
            "Config source is required".to_string(),
        ))?;
        Ok(ConfigManager::new(storage, config))
    }
}
