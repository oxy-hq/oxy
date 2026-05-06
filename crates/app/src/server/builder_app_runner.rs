//! Implements [`BuilderAppRunner`] for the builder copilot by delegating to
//! the Oxy app service.  Lives in `oxy-app` so it can access `AppService`
//! without creating a circular dependency in the lower crates.

use std::collections::HashMap;
use std::path::Path;

use agentic_builder::BuilderAppRunner;
use oxy::adapters::workspace::builder::WorkspaceBuilder;
use oxy::execute::types::{Data, DataContainer};
use serde_json::{Value, json};

use crate::server::service::app::AppService;

const MAX_SAMPLE_ROWS: usize = 10;
const MAX_LIST_ITEMS: usize = 20;

/// [`BuilderAppRunner`] that executes `.app.yml` files via the Oxy app service.
pub struct OxyAppRunner;

#[async_trait::async_trait]
impl BuilderAppRunner for OxyAppRunner {
    async fn run_app(
        &self,
        workspace_root: &Path,
        app_file: &str,
        params: HashMap<String, Value>,
    ) -> Result<Value, String> {
        let workspace_manager = WorkspaceBuilder::new(uuid::Uuid::new_v4())
            .with_workspace_path(workspace_root)
            .await
            .map_err(|e| e.to_string())?
            .build()
            .await
            .map_err(|e| e.to_string())?;

        let app_path = workspace_root.join(app_file);
        let mut app_service = AppService::new(workspace_manager);
        let data = app_service
            .run(&app_path, params)
            .await
            .map_err(|e| e.to_string())?;

        Ok(summarize_data_container(&data))
    }
}

fn summarize_data_container(container: &DataContainer) -> Value {
    match container {
        DataContainer::Map(map) => {
            let tasks: Vec<Value> = map
                .iter()
                .map(|(task_name, data)| {
                    json!({
                        "task": task_name,
                        "result": summarize_data_container(data),
                    })
                })
                .collect();
            json!({ "tasks": tasks })
        }
        DataContainer::List(items) => {
            let summarized: Vec<Value> = items
                .iter()
                .take(MAX_LIST_ITEMS)
                .map(summarize_data_container)
                .collect();
            Value::Array(summarized)
        }
        DataContainer::Single(data) => summarize_data(data),
        DataContainer::None => json!({ "status": "no_data" }),
    }
}

fn summarize_data(data: &Data) -> Value {
    match data {
        Data::Table(table_data) => {
            // Use the pre-serialized JSON when available (populated at write time).
            if let Some(json_str) = table_data.json.as_deref() {
                if let Ok(Value::Array(rows)) = serde_json::from_str::<Value>(json_str) {
                    let total_rows = rows.len();
                    let sample: Vec<Value> = rows.into_iter().take(MAX_SAMPLE_ROWS).collect();
                    return json!({
                        "status": "ok",
                        "total_rows": total_rows,
                        "sample_rows": sample,
                    });
                }
            }
            // Fallback: no inline JSON — report row count as unknown.
            json!({ "status": "ok", "note": "table written to parquet (no inline sample)" })
        }
        Data::Text(text) => json!({ "status": "ok", "text": text }),
        Data::Bool(b) => json!({ "status": "ok", "value": b }),
        Data::None => json!({ "status": "no_data" }),
    }
}
