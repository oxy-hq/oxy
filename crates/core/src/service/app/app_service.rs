use super::cache::AppCache;
use super::types::{AppResult, TASKS_KEY};
use crate::adapters::project::manager::ProjectManager;
use crate::config::model::{AppConfig, Task};
use crate::errors::OxyError;
use crate::execute::types::DataContainer;
use crate::service::workflow::WorkflowEventHandler;
use crate::workflow::WorkflowLauncher;
use crate::workflow::loggers::NoopLogger;
use std::path::PathBuf;

pub struct AppService {
    project_manager: ProjectManager,
    cache: AppCache,
}

impl AppService {
    pub fn new(project_manager: ProjectManager) -> Self {
        let config_manager = project_manager.config_manager.clone();
        Self {
            project_manager,
            cache: AppCache::new(config_manager),
        }
    }

    pub async fn get_config(&self, app_path: &PathBuf) -> AppResult<AppConfig> {
        let config_manager = &self.project_manager.config_manager;
        let app = config_manager.resolve_app(app_path).await?;
        Ok(app)
    }

    pub async fn get_tasks(&self, app_path: &PathBuf) -> AppResult<Vec<Task>> {
        let yaml_content = self.read_yaml_file(app_path).await?;
        let root_map = self.parse_yaml_to_mapping(&yaml_content)?;

        let tasks_value = root_map
            .get(&serde_yaml::Value::String(TASKS_KEY.to_string()))
            .ok_or_else(|| {
                OxyError::ConfigurationError("No tasks found in app config".to_string())
            })?;

        serde_yaml::from_value(tasks_value.clone())
            .map_err(|e| OxyError::ConfigurationError(format!("Failed to parse tasks: {e}")))
    }

    pub async fn run(&mut self, app_path: &PathBuf) -> AppResult<DataContainer> {
        tracing::info!("Running app: {app_path:?}");

        let tasks = self.get_tasks(app_path).await?;
        self.cache.clean_up_data(app_path, &tasks).await?;

        let output_container = WorkflowLauncher::new()
            .with_project(self.project_manager.clone())
            .await?
            .launch_tasks(tasks.clone(), WorkflowEventHandler::new(NoopLogger {}))
            .await?;

        let data = self
            .cache
            .save_data(app_path, &tasks, output_container)
            .await?;
        Ok(data)
    }

    pub async fn try_load_cached_data(
        &self,
        app_path: &PathBuf,
        tasks: &[Task],
    ) -> Option<DataContainer> {
        self.cache.try_load_data(app_path, tasks).await
    }

    pub async fn read_yaml_file(&self, path: &PathBuf) -> AppResult<String> {
        let config_manager = &self.project_manager.config_manager;
        let full_path = config_manager.resolve_file(path).await.map_err(|e| {
            tracing::debug!("Failed to resolve file: {:?} {}", path, e);
            OxyError::ConfigurationError(format!("Failed to resolve file: {e}"))
        })?;

        std::fs::read_to_string(&full_path).map_err(|e| {
            tracing::info!("Failed to read file: {:?}", e);
            OxyError::ConfigurationError(format!("Failed to read file: {e}"))
        })
    }

    fn parse_yaml_to_mapping(&self, yaml_content: &str) -> AppResult<serde_yaml::Mapping> {
        let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml_content)
            .map_err(|e| OxyError::ConfigurationError(format!("Failed to parse YAML: {e}")))?;

        match yaml_value {
            serde_yaml::Value::Mapping(map) => Ok(map),
            _ => Err(OxyError::ConfigurationError(
                "Expected YAML object at root".to_string(),
            )),
        }
    }
}
