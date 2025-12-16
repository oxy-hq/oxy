use std::path::{Path, PathBuf};
use tokio::fs;

use crate::{
    agent::builders::fsm::config::AgenticConfig, constants::UNPUBLISH_APP_DIR, errors::OxyError,
};

use super::model::{AgentConfig, AppConfig, Config, Workflow, WorkflowWithRawVariables};

const DEFAULT_CONFIG_PATH: &str = "config.yml";
const WORKFLOW_EXTENSION: &str = ".workflow";
const AGENT_EXTENSION: &str = ".agent";
const AGENTIC_WORKFLOW_EXTENSION: &str = ".aw";

#[enum_dispatch::enum_dispatch]
pub(super) trait ConfigStorage {
    async fn load_config(&self) -> Result<Config, OxyError>;
    async fn load_config_with_fallback(&self) -> Config;
    async fn load_agent_config<P: AsRef<Path>>(
        &self,
        agent_ref: P,
    ) -> Result<AgentConfig, OxyError>;
    async fn load_agentic_workflow_config<P: AsRef<Path>>(
        &self,
        agent_ref: P,
    ) -> Result<AgenticConfig, OxyError>;
    async fn load_workflow_config<P: AsRef<Path>>(
        &self,
        workflow_ref: P,
    ) -> Result<Workflow, OxyError>;
    async fn load_workflow_config_with_raw_variables<P: AsRef<Path>>(
        &self,
        workflow_ref: P,
    ) -> Result<WorkflowWithRawVariables, OxyError>;
    async fn fs_link<P: AsRef<Path>>(&self, file_ref: P) -> Result<String, OxyError>;
    async fn resolve_state_dir(&self) -> Result<PathBuf, OxyError>;
    async fn glob<P: AsRef<Path>>(&self, path: P) -> Result<Vec<String>, OxyError>;
    async fn list_agents(&self) -> Result<Vec<PathBuf>, OxyError>;
    async fn list_agentic_workflows(&self) -> Result<Vec<PathBuf>, OxyError>;
    async fn list_apps(&self) -> Result<Vec<PathBuf>, OxyError>;
    async fn list_workflows(&self) -> Result<Vec<PathBuf>, OxyError>;
    async fn load_app_config<P: AsRef<Path>>(&self, app_path: P) -> Result<AppConfig, OxyError>;
    async fn get_charts_dir(&self) -> Result<PathBuf, OxyError>;
}

#[derive(Debug)]
#[enum_dispatch::enum_dispatch(ConfigStorage)]
pub(super) enum ConfigSource {
    LocalSource,
}

impl ConfigSource {
    pub fn local<P: AsRef<Path>>(project_path: P) -> Result<Self, OxyError> {
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
    pub fn new<P: AsRef<Path>>(project_path: P) -> Result<Self, OxyError> {
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
                        .map(|s| s.ends_with(format!(".{sub_extension}.yml").as_str()))
                        .unwrap_or(false)
                {
                    files.push(path);
                }
            }
        }
        files
    }

    fn ensure_dir_exists(&self, path: &Path) {
        if !path.exists()
            && let Err(e) = std::fs::create_dir_all(path)
        {
            eprintln!("Error: Could not create directory: {e}");
            std::process::exit(1);
        }
    }

    fn validate_path_within_project<P: AsRef<Path>>(
        &self,
        file_ref: P,
    ) -> Result<PathBuf, OxyError> {
        let resolved_path = self.project_path.join(file_ref);
        let canonical_project = self
            .project_path
            .canonicalize()
            .map_err(|e| OxyError::IOError(format!("Failed to canonicalize project path: {e}")))?;

        let canonical_resolved = if resolved_path.exists() {
            resolved_path.canonicalize().map_err(|e| {
                OxyError::IOError(format!("Failed to canonicalize resolved path: {e}"))
            })?
        } else {
            let parent = resolved_path.parent().ok_or_else(|| {
                OxyError::IOError("Invalid path: no parent directory".to_string())
            })?;
            let filename = resolved_path
                .file_name()
                .ok_or_else(|| OxyError::IOError("Invalid path: no filename".to_string()))?;

            if parent.exists() {
                parent
                    .canonicalize()
                    .map_err(|e| {
                        OxyError::IOError(format!("Failed to canonicalize parent path: {e}"))
                    })?
                    .join(filename)
            } else {
                let normalized = self.normalize_path(&resolved_path);
                let normalized_project = self.normalize_path(&self.project_path);
                if !normalized.starts_with(&normalized_project) {
                    return Err(OxyError::IOError(
                        "Path traversal detected: resolved path is outside project directory"
                            .to_string(),
                    ));
                }
                return Ok(resolved_path);
            }
        };

        if !canonical_resolved.starts_with(&canonical_project) {
            return Err(OxyError::IOError(
                "Path traversal detected: resolved path is outside project directory".to_string(),
            ));
        }

        Ok(resolved_path)
    }

    fn normalize_path(&self, path: &Path) -> PathBuf {
        let mut components = Vec::new();
        for component in path.components() {
            match component {
                std::path::Component::Normal(name) => components.push(name),
                std::path::Component::ParentDir => {
                    components.pop();
                }
                std::path::Component::CurDir => {}
                other => components.push(other.as_os_str()),
            }
        }
        components.iter().collect()
    }
}

impl ConfigStorage for LocalSource {
    async fn load_config(&self) -> Result<Config, OxyError> {
        let resolved_path = PathBuf::from(&self.project_path).join(&self.config_path);
        let config_yml = fs::read_to_string(resolved_path).await.map_err(|e| {
            OxyError::ConfigurationError(format!(
                "Failed to read config from file: {e}, project_path: {}",
                self.project_path.display()
            ))
        })?;
        let mut config: Config = serde_yaml::from_str(&config_yml).map_err(|e| {
            OxyError::ConfigurationError(format!(
                "Failed to deserialize config: {e}, project_path: {}",
                self.project_path.display()
            ))
        })?;
        config.project_path = self.project_path.clone();
        Ok(config)
    }

    async fn load_config_with_fallback(&self) -> Config {
        let resolved_path = PathBuf::from(&self.project_path).join(&self.config_path);
        let config_yml = std::fs::read_to_string(resolved_path).unwrap_or_default();
        let mut config: Config = serde_yaml::from_str(&config_yml).unwrap_or_else(|_| Config {
            defaults: None,
            project_path: self.project_path.clone(),
            models: [].to_vec(),
            databases: [].to_vec(),
            builder_agent: None,
            integrations: vec![],
            slack: None,
            mcp: None,
        });
        config.project_path = self.project_path.clone();
        config
    }

    async fn load_agent_config<P: AsRef<Path>>(
        &self,
        agent_ref: P,
    ) -> Result<AgentConfig, OxyError> {
        let resolved_path = self.validate_path_within_project(agent_ref)?;
        let agent_yml = fs::read_to_string(&resolved_path).await.map_err(|e| {
            OxyError::ConfigurationError(format!("Failed to read agent config from file: {e}"))
        })?;
        let mut agent_config: AgentConfig = serde_yaml::from_str(&agent_yml).map_err(|e| {
            OxyError::ConfigurationError(format!(
                "Failed to deserialize agent {} config: {e}",
                resolved_path.display()
            ))
        })?;
        if agent_config.name.is_empty() {
            agent_config.name = self.get_stem_by_extension(&resolved_path, AGENT_EXTENSION);
        }
        Ok(agent_config)
    }

    async fn load_agentic_workflow_config<P: AsRef<Path>>(
        &self,
        agent_ref: P,
    ) -> Result<AgenticConfig, OxyError> {
        let resolved_path = PathBuf::from(&self.project_path).join(agent_ref);
        let agent_yml = fs::read_to_string(&resolved_path).await.map_err(|e| {
            OxyError::ConfigurationError(format!("Failed to read agent config from file: {e}"))
        })?;
        let mut agent_config: AgenticConfig = serde_yaml::from_str(&agent_yml).map_err(|e| {
            OxyError::ConfigurationError(format!("Failed to deserialize agent config: {e}"))
        })?;
        if agent_config.name.is_empty() {
            agent_config.name =
                self.get_stem_by_extension(&resolved_path, AGENTIC_WORKFLOW_EXTENSION);
        }
        Ok(agent_config)
    }

    async fn load_workflow_config<P: AsRef<Path>>(
        &self,
        workflow_ref: P,
    ) -> Result<Workflow, OxyError> {
        let resolved_path = self.validate_path_within_project(workflow_ref)?;
        let workflow_yml = fs::read_to_string(&resolved_path).await.map_err(|e| {
            OxyError::ConfigurationError(format!("Failed to read workflow config from file: {e}"))
        })?;
        let mut workflow_config: Workflow = serde_yaml::from_str(&workflow_yml).map_err(|e| {
            OxyError::ConfigurationError(format!("Failed to deserialize workflow config: {e}"))
        })?;
        workflow_config.name = self.get_stem_by_extension(&resolved_path, WORKFLOW_EXTENSION);
        Ok(workflow_config)
    }

    async fn load_workflow_config_with_raw_variables<P: AsRef<Path>>(
        &self,
        workflow_ref: P,
    ) -> Result<WorkflowWithRawVariables, OxyError> {
        let resolved_path = self.validate_path_within_project(workflow_ref)?;
        let workflow_yml = fs::read_to_string(&resolved_path).await.map_err(|e| {
            OxyError::ConfigurationError(format!("Failed to read workflow config from file: {e}"))
        })?;
        let mut temp_workflow: WorkflowWithRawVariables = serde_yaml::from_str(&workflow_yml)
            .map_err(|e| {
                OxyError::ConfigurationError(format!("Failed to deserialize workflow config: {e}"))
            })?;
        temp_workflow.name = self.get_stem_by_extension(&resolved_path, WORKFLOW_EXTENSION);
        Ok(temp_workflow)
    }

    async fn fs_link<P: AsRef<Path>>(&self, file_ref: P) -> Result<String, OxyError> {
        let resolved_path = self.validate_path_within_project(file_ref)?;
        Ok(resolved_path.display().to_string())
    }

    async fn resolve_state_dir(&self) -> Result<PathBuf, OxyError> {
        let path = PathBuf::from(&self.project_path).join(".oxy_state");
        self.ensure_dir_exists(&path);
        Ok(path)
    }

    async fn glob<P: AsRef<Path>>(&self, path: P) -> Result<Vec<String>, OxyError> {
        let path = self.project_path.join(path);
        let pattern = path.to_str().unwrap();
        let glob = glob::glob(pattern).map_err(|err| {
            OxyError::IOError(format!("Failed to expand glob pattern '{pattern}': {err}"))
        })?;
        Ok(glob
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.is_file())
            .map(|entry| entry.display().to_string())
            .collect())
    }

    async fn list_agents(&self) -> Result<Vec<PathBuf>, OxyError> {
        Ok(self.list_by_sub_extension(None, "agent"))
    }

    async fn list_agentic_workflows(&self) -> Result<Vec<PathBuf>, OxyError> {
        Ok(self.list_by_sub_extension(None, "aw"))
    }

    async fn list_workflows(&self) -> Result<Vec<PathBuf>, OxyError> {
        let mut workflows = self.list_by_sub_extension(None, "workflow");
        workflows.extend(self.list_by_sub_extension(None, "automation"));
        Ok(workflows)
    }

    async fn list_apps(&self) -> Result<Vec<PathBuf>, OxyError> {
        let apps = self.list_by_sub_extension(None, "app");
        let project_path = self.project_path.clone();
        Ok(apps
            .iter()
            .filter(|path| {
                path.strip_prefix(&project_path)
                    .map(|p| {
                        !p.to_string_lossy()
                            .starts_with(&format!("{UNPUBLISH_APP_DIR}/"))
                    })
                    .unwrap_or(true)
            })
            .cloned()
            .collect())
    }

    async fn load_app_config<P: AsRef<Path>>(&self, app_path: P) -> Result<AppConfig, OxyError> {
        let resolved_path = self.validate_path_within_project(app_path)?;
        let agent_yml = fs::read_to_string(&resolved_path).await.map_err(|e| {
            OxyError::ConfigurationError(format!("Failed to read agent config from file: {e}"))
        })?;
        let app_config: AppConfig = serde_yaml::from_str(&agent_yml).map_err(|e| {
            OxyError::ConfigurationError(format!("Failed to deserialize agent config: {e}"))
        })?;

        Ok(app_config)
    }

    async fn get_charts_dir(&self) -> Result<PathBuf, OxyError> {
        let charts_dir = self.resolve_state_dir().await?.join("charts");
        self.ensure_dir_exists(&charts_dir);
        Ok(charts_dir)
    }
}
