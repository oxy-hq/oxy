use super::cache::AppCache;
use super::types::{AppResult, TASKS_KEY};
use crate::agentic_wiring::OxyProjectContext;
use oxy::adapters::workspace::manager::WorkspaceManager;
use oxy::config::model::{AppConfig, ControlConfig, Display, Task};
use oxy::execute::renderer::Renderer;
use oxy::execute::types::{Data, DataContainer, TableData};
use oxy_shared::errors::OxyError;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

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
    workspace_manager: WorkspaceManager,
    cache: AppCache,
}

impl AppService {
    pub fn new(workspace_manager: WorkspaceManager) -> Self {
        let config_manager = workspace_manager.config_manager.clone();
        Self {
            workspace_manager,
            cache: AppCache::new(config_manager),
        }
    }

    pub async fn get_config(&self, app_path: &PathBuf) -> AppResult<AppConfig> {
        let config_manager = &self.workspace_manager.config_manager;
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

        // Data Apps run a free-form `Vec<oxy::Task>` (from `.app.yml`)
        // synchronously and need the final results back to render charts /
        // tables on the same request. We feed the tasks through
        // `agentic_pipeline::workflow_run::run_inline_workflow`, which drives
        // the workflow decider in-process without the coordinator queue —
        // round-trip the tasks through JSON to land on
        // `agentic_workflow::WorkflowConfig`. Both `Task` types use
        // `#[serde(tag = "type")]` so the shapes line up.
        let _ = controls; // controls render via `convert_to_data` below
        let workflow_value = serde_json::json!({
            "name": "app-tasks-inline",
            "tasks": serde_json::to_value(&tasks).map_err(|e| {
                OxyError::RuntimeError(format!("serialize app tasks: {e}"))
            })?,
        });
        let workflow_config: agentic_workflow::WorkflowConfig =
            serde_json::from_value(workflow_value).map_err(|e| {
                OxyError::RuntimeError(format!("convert app tasks → WorkflowConfig: {e}"))
            })?;

        let agent_runner =
            crate::agentic_wiring::OxyInlineAgentRunner::new(self.workspace_manager.clone());
        let project_ctx = Arc::new(OxyProjectContext::new(self.workspace_manager.clone()));
        let workspace: Arc<dyn agentic_workflow::WorkspaceContext> = project_ctx;
        let results = agentic_pipeline::workflow_run::run_inline_workflow_with(
            workspace.as_ref(),
            workflow_config,
            None,
            Some(&agent_runner),
        )
        .await
        .map_err(|e| OxyError::RuntimeError(format!("app inline workflow: {e}")))?;

        // Build the cache-layer `DataContainer` directly. The frontend
        // (`AppPreview` / `registerFromTableData` in
        // `web-app/src/components/AppPreview/Displays/utils.ts`) expects
        // tabular task results in `{file_path, json}` shape with `json`
        // as an array-of-objects string — that's what
        // `Data::Table(TableData { ... })` serialises to.
        //
        // The agentic-workflow runner produces tabular results as
        // `{columns, rows}` (the standard step-executor output);
        // `workflow_task_to_data` converts that to array-of-objects
        // and wraps in `TableData`. Non-tabular results fall through
        // as `Data::Text(stringified)`.
        //
        // Bypasses the legacy `OutputContainer → to_data` path
        // entirely — that path expects arrow batches to write parquet
        // shards, which we don't have here.
        let mut map: HashMap<String, DataContainer> = HashMap::with_capacity(results.len());
        for task in &tasks {
            if let Some(value) = results.get(&task.name) {
                let data = workflow_task_to_data(&task.name, value);
                map.insert(task.name.clone(), DataContainer::Single(data));
            }
        }
        let data_container = DataContainer::Map(map);

        if has_params {
            let data = self
                .cache
                .convert_to_data_container(app_path, &tasks, &params, data_container)
                .await?;
            return Ok(data);
        }

        let data = self
            .cache
            .save_data_container(app_path, &tasks, data_container)
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
        let config_manager = &self.workspace_manager.config_manager;
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

/// Convert an inline-workflow task result into a [`Data`] suitable for
/// the frontend's `AppPreview` consumer.
///
/// Two shapes coming out of the agentic-workflow runner:
/// - **Tabular** — `{columns: [...], rows: [[...]]}` from `execute_sql`,
///   `semantic_query`, `omni_query`. Reshape to array-of-objects JSON
///   and wrap in [`Data::Table`] with a synthetic `file_path`. The
///   frontend reads `tableData.json` directly into DuckDB-WASM; the
///   path is only used as a stable view name.
/// - **Anything else** — stringify and wrap in [`Data::Text`]. Covers
///   formatter / agent outputs.
fn workflow_task_to_data(task_name: &str, value: &JsonValue) -> Data {
    if let Some(records) = tabular_to_records(value) {
        let json = serde_json::to_string(&records).ok();
        let file_path = PathBuf::from(format!("app_inline/{task_name}.parquet"));
        return Data::Table(TableData { file_path, json });
    }
    match value {
        JsonValue::String(s) => Data::Text(s.clone()),
        other => Data::Text(other.to_string()),
    }
}

/// Re-shape a `{columns: [...], rows: [[...]]}` step result as an
/// `[{col: val, ...}, ...]` array-of-objects, matching the shape the
/// frontend's DuckDB-WASM `read_json_auto` ingest expects. Returns
/// `None` when the value isn't tabular.
fn tabular_to_records(value: &JsonValue) -> Option<Vec<JsonValue>> {
    let columns = value.get("columns")?.as_array()?;
    let rows = value.get("rows")?.as_array()?;
    let col_names: Vec<&str> = columns.iter().filter_map(|v| v.as_str()).collect();
    let records = rows
        .iter()
        .filter_map(|row| {
            let cells = row.as_array()?;
            let mut obj = serde_json::Map::with_capacity(col_names.len());
            for (i, name) in col_names.iter().enumerate() {
                let cell = cells.get(i).cloned().unwrap_or(JsonValue::Null);
                obj.insert((*name).to_string(), cell);
            }
            Some(JsonValue::Object(obj))
        })
        .collect();
    Some(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tabular_value_becomes_array_of_objects() {
        let v = json!({
            "columns": ["month", "total"],
            "rows": [["2010-02", 42.0], ["2010-03", 31.5]],
            "sql": "select ...",
        });
        let records = tabular_to_records(&v).unwrap();
        assert_eq!(
            records,
            vec![
                json!({"month": "2010-02", "total": 42.0}),
                json!({"month": "2010-03", "total": 31.5}),
            ]
        );
    }

    #[test]
    fn missing_rows_or_columns_returns_none() {
        assert!(tabular_to_records(&json!({"rows": []})).is_none());
        assert!(tabular_to_records(&json!({"columns": ["x"]})).is_none());
        assert!(tabular_to_records(&json!("hello")).is_none());
    }

    #[test]
    fn tabular_task_produces_data_table_with_json_string() {
        let v = json!({
            "columns": ["a"],
            "rows": [[1], [2]],
        });
        let data = workflow_task_to_data("query", &v);
        let Data::Table(td) = data else {
            panic!("expected Data::Table");
        };
        assert!(td.file_path.to_string_lossy().contains("query"));
        let json: JsonValue = serde_json::from_str(&td.json.unwrap()).unwrap();
        assert_eq!(json, json!([{"a": 1}, {"a": 2}]));
    }

    #[test]
    fn non_tabular_task_produces_data_text() {
        let v = json!({"text": "hello"});
        let data = workflow_task_to_data("greet", &v);
        match data {
            Data::Text(s) => assert!(s.contains("hello")),
            other => panic!("expected Data::Text, got {other:?}"),
        }
    }

    /// Cells that have fewer elements than columns pad with null —
    /// keeps DuckDB-WASM `read_json_auto` happy with rectangular input.
    #[test]
    fn short_rows_pad_with_null() {
        let v = json!({"columns": ["a", "b"], "rows": [[1]]});
        let records = tabular_to_records(&v).unwrap();
        assert_eq!(records, vec![json!({"a": 1, "b": null})]);
    }
}
