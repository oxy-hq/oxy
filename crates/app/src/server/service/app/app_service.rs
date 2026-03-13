use super::cache::AppCache;
use super::types::{AppResult, TASKS_KEY};
use crate::server::service::workflow::WorkflowEventHandler;
use oxy::adapters::project::manager::ProjectManager;
use oxy::config::model::{AppConfig, ControlConfig, Display, Task};
use oxy::execute::renderer::Renderer;
use oxy::execute::types::DataContainer;
use oxy_shared::errors::OxyError;
use oxy_workflow::builders::WorkflowLauncher;
use oxy_workflow::loggers::NoopLogger;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::PathBuf;

/// Render Jinja expressions inside a control field value (e.g. `default`).
///
/// Supports global functions such as `now()`:
///
/// ```yaml
/// - type: control
///   name: start_date
///   control_type: date
///   default: "{{ now(fmt='%Y-%m-%d') }}"
/// ```
///
/// Non-string values and strings without Jinja tokens are returned unchanged.
/// Rendering errors are logged as warnings and the original value is returned.
pub fn render_control_default(val: JsonValue) -> JsonValue {
    let JsonValue::String(ref s) = val else {
        return val;
    };
    if !s.contains("{{") && !s.contains("{%") {
        return val;
    }
    let renderer = Renderer::new(minijinja::Value::UNDEFINED);
    match renderer.render_str(s) {
        Ok(rendered) => JsonValue::String(rendered),
        Err(e) => {
            tracing::warn!("Failed to render Jinja in control default '{s}': {e}");
            val
        }
    }
}

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
            .get(serde_yaml::Value::String(TASKS_KEY.to_string()))
            .ok_or_else(|| {
                OxyError::ConfigurationError("No tasks found in app config".to_string())
            })?;

        serde_yaml::from_value(tasks_value.clone())
            .map_err(|e| OxyError::ConfigurationError(format!("Failed to parse tasks: {e}")))
    }

    pub async fn run(
        &mut self,
        app_path: &PathBuf,
        params: HashMap<String, JsonValue>,
    ) -> AppResult<DataContainer> {
        tracing::info!("Running app: {app_path:?}");

        let config = self.get_config(app_path).await?;

        // Collect all declared controls: top-level `controls:` plus any inline
        // `- type: control` / `- type: controls` items from the `display:` list.
        let mut all_controls: Vec<ControlConfig> = config.controls.clone();
        for display in &config.display {
            match display {
                Display::Control(c) => all_controls.push(ControlConfig::from(c.clone())),
                Display::Controls(cs) => all_controls.extend(cs.items.iter().cloned()),
                _ => {}
            }
        }

        // Build controls context: config defaults overridden by user-provided params.
        // Empty-string param values are treated as absent so the configured default is used.
        let controls: HashMap<String, JsonValue> = all_controls
            .iter()
            .map(|c| {
                let param = params.get(&c.name).and_then(|v| {
                    // Treat empty string as absent — avoids injecting '' into typed SQL columns.
                    if v.as_str() == Some("") {
                        None
                    } else {
                        Some(v.clone())
                    }
                });
                let val = render_control_default(
                    param
                        .or_else(|| c.default.clone())
                        .unwrap_or(JsonValue::Null),
                );
                (c.name.clone(), val)
            })
            .collect();

        // Reuse the already-parsed config instead of re-reading and re-parsing the YAML file.
        let tasks = config.tasks;

        let has_params = !params.is_empty();
        if !has_params {
            self.cache.clean_up_data(app_path, &tasks).await?;
        }

        let output_container = WorkflowLauncher::new()
            .with_controls(controls)
            .with_project(self.project_manager.clone())
            .await?
            .launch_tasks(tasks.clone(), WorkflowEventHandler::new(NoopLogger {}))
            .await?;

        if has_params {
            // Write to a params-specific path; don't overwrite the default cache.
            let data = self
                .cache
                .convert_to_data(app_path, &tasks, &params, output_container)
                .await?;
            return Ok(data);
        }

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
