use super::types::{AppResult, TASKS_KEY};
use super::utils::{map_config_error, yaml_string_value};
use crate::config::ConfigBuilder;
use crate::config::model::{AppConfig, Task};
use crate::project::resolve_project_path;
use std::path::PathBuf;

pub async fn create_config_builder() -> AppResult<ConfigBuilder> {
    let project_path = resolve_project_path()?;
    ConfigBuilder::new()
        .with_project_path(project_path)
        .map_err(|e| {
            crate::errors::OxyError::ConfigurationError(format!(
                "Failed to create config builder: {e}"
            ))
        })
}

pub async fn get_app_config(path: &PathBuf) -> AppResult<AppConfig> {
    let config_builder = create_config_builder().await?;
    let config = config_builder.build().await?;
    let app = config.resolve_app(path).await?;
    Ok(app)
}

pub async fn get_app_tasks(path: &PathBuf) -> AppResult<Vec<Task>> {
    let yaml_content = read_yaml_file(path).await?;
    let root_map = parse_yaml_to_mapping(&yaml_content)?;

    let tasks_value = root_map.get(&yaml_string_value(TASKS_KEY)).ok_or_else(|| {
        crate::errors::OxyError::ConfigurationError("No tasks found in app config".to_string())
    })?;

    serde_yaml::from_value(tasks_value.clone()).map_err(map_config_error("Failed to parse tasks"))
}

pub async fn read_yaml_file(path: &PathBuf) -> AppResult<String> {
    let project_path = resolve_project_path()?;
    let resolved_path = project_path.join(path);

    std::fs::read_to_string(&resolved_path)
        .map_err(map_config_error("Failed to read app config from file"))
}

pub async fn read_yaml_file_with_config(path: &PathBuf) -> AppResult<String> {
    let config_builder = create_config_builder().await?;
    let config = config_builder.build().await?;

    let full_path = config.resolve_file(path).await.map_err(|e| {
        tracing::debug!("Failed to resolve file: {:?} {}", path, e);
        map_config_error("Failed to resolve file")(e)
    })?;

    std::fs::read_to_string(&full_path).map_err(|e| {
        tracing::info!("Failed to read file: {:?}", e);
        map_config_error("Failed to read file")(e)
    })
}

pub fn parse_yaml_to_mapping(yaml_content: &str) -> AppResult<serde_yaml::Mapping> {
    let yaml_value: serde_yaml::Value =
        serde_yaml::from_str(yaml_content).map_err(map_config_error("Failed to parse YAML"))?;

    match yaml_value {
        serde_yaml::Value::Mapping(map) => Ok(map),
        _ => Err(crate::errors::OxyError::ConfigurationError(
            "Expected YAML object at root".to_string(),
        )),
    }
}
