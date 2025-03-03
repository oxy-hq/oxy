use std::path::{Path, PathBuf};
use tokio::fs;

use crate::errors::OnyxError;

use super::model::{AgentConfig, Config, Workflow};

const DEFAULT_CONFIG_PATH: &str = "config.yml";
const WORKFLOW_EXTENSION: &str = ".workflow";
const AGENT_EXTENSION: &str = ".agent";

#[enum_dispatch::enum_dispatch]
pub(super) trait ConfigStorage {
    async fn load_config(&self) -> Result<Config, OnyxError>;
    async fn load_agent_config<P: AsRef<Path>>(
        &self,
        agent_ref: P,
    ) -> Result<AgentConfig, OnyxError>;
    async fn load_workflow_config<P: AsRef<Path>>(
        &self,
        workflow_ref: P,
    ) -> Result<Workflow, OnyxError>;
    async fn fs_link<P: AsRef<Path>>(&self, file_ref: P) -> Result<String, OnyxError>;
    async fn glob<P: AsRef<Path>>(&self, path: P) -> Result<Vec<String>, OnyxError>;
    async fn list_agents(&self) -> Result<Vec<PathBuf>, OnyxError>;
}

#[derive(Debug)]
#[enum_dispatch::enum_dispatch(ConfigStorage)]
pub(super) enum ConfigSource {
    LocalSource,
}

impl ConfigSource {
    pub fn local<P: AsRef<Path>>(project_path: P) -> Result<Self, OnyxError> {
        let local_source = LocalSource::new(project_path)?;
        Ok(ConfigSource::LocalSource(local_source))
    }
}

#[derive(Debug)]
pub(super) struct LocalSource {
    project_path: PathBuf,
    config_path: String,
}

impl LocalSource {
    pub fn new<P: AsRef<Path>>(project_path: P) -> Result<Self, OnyxError> {
        Ok(LocalSource {
            project_path: project_path.as_ref().to_path_buf(),
            config_path: DEFAULT_CONFIG_PATH.to_string(),
        })
    }

    fn get_stem_by_extension(&self, path: &PathBuf, extension: &str) -> String {
        let file_stem = path.file_stem().unwrap().to_str().unwrap();
        file_stem
            .strip_suffix(extension)
            .unwrap_or(file_stem)
            .to_string()
    }

    fn list_by_sub_extension(&self, dir: Option<&PathBuf>, sub_extension: &str) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let dir = dir.unwrap_or(&self.project_path);
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    files.extend(self.list_by_sub_extension(Some(&path), sub_extension));
                } else if path.is_file()
                    && path.extension().and_then(|s| s.to_str()) == Some("yml")
                    && path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .map(|s| s.ends_with(format!(".{}.yml", sub_extension).as_str()))
                        .unwrap_or(false)
                {
                    files.push(path);
                }
            }
        }

        files
    }
}

impl ConfigStorage for LocalSource {
    async fn load_config(&self) -> Result<Config, OnyxError> {
        let resolved_path = PathBuf::from(&self.project_path).join(&self.config_path);
        let config_yml = fs::read_to_string(resolved_path).await.map_err(|e| {
            OnyxError::ConfigurationError(format!("Failed to read config from file: {e}"))
        })?;
        let config: Config = serde_yaml::from_str(&config_yml).map_err(|e| {
            OnyxError::ConfigurationError(format!("Failed to deserialize config: {e}"))
        })?;
        Ok(config)
    }

    async fn load_agent_config<P: AsRef<Path>>(
        &self,
        agent_ref: P,
    ) -> Result<AgentConfig, OnyxError> {
        let resolved_path = PathBuf::from(&self.project_path).join(agent_ref);
        let agent_yml = fs::read_to_string(&resolved_path).await.map_err(|e| {
            OnyxError::ConfigurationError(format!("Failed to read agent config from file: {e}"))
        })?;
        let mut agent_config: AgentConfig = serde_yaml::from_str(&agent_yml).map_err(|e| {
            OnyxError::ConfigurationError(format!("Failed to deserialize agent config: {e}"))
        })?;
        agent_config.name = self.get_stem_by_extension(&resolved_path, AGENT_EXTENSION);
        Ok(agent_config)
    }

    async fn load_workflow_config<P: AsRef<Path>>(
        &self,
        workflow_ref: P,
    ) -> Result<Workflow, OnyxError> {
        let resolved_path = PathBuf::from(&self.project_path).join(workflow_ref);
        let workflow_yml = fs::read_to_string(&resolved_path).await.map_err(|e| {
            OnyxError::ConfigurationError(format!("Failed to read workflow config from file: {e}"))
        })?;
        let mut workflow_config: Workflow = serde_yaml::from_str(&workflow_yml).map_err(|e| {
            OnyxError::ConfigurationError(format!("Failed to deserialize workflow config: {e}"))
        })?;
        workflow_config.name = self.get_stem_by_extension(&resolved_path, WORKFLOW_EXTENSION);
        Ok(workflow_config)
    }

    async fn fs_link<P: AsRef<Path>>(&self, file_ref: P) -> Result<String, OnyxError> {
        let resolved_path = PathBuf::from(&self.project_path).join(file_ref);
        Ok(resolved_path.display().to_string())
    }

    async fn glob<P: AsRef<Path>>(&self, path: P) -> Result<Vec<String>, OnyxError> {
        let path = self.project_path.join(path);
        let pattern = path.to_str().unwrap();
        let glob = glob::glob(pattern).map_err(|err| {
            OnyxError::IOError(format!(
                "Failed to expand glob pattern '{}': {}",
                pattern, err
            ))
        })?;
        Ok(glob
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.is_file())
            .map(|entry| entry.display().to_string())
            .collect())
    }

    async fn list_agents(&self) -> Result<Vec<PathBuf>, OnyxError> {
        Ok(self.list_by_sub_extension(None, "agent"))
    }
}
