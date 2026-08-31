//! `BuilderAppRunner` implementation backed by Oxy's `AppService`.
//!
//! Backs the `run_app` tool. Onboarding's App / App2 phases call `run_app`
//! after `write_file` so a malformed-SQL dashboard never reaches the user
//! as a blank screen. Schema validation (`validate_project`) only catches
//! structural YAML errors; this runner exercises the full task pipeline
//! (`AppService::run`) so dialect-specific runtime SQL errors surface
//! during the build, not at first dashboard load.
//!
//! Mirrors [`crate::server::builder_test_runner::OxyTestRunner`]: a stateless
//! singleton that rebuilds a `WorkspaceManager` from the workspace_root the
//! tool dispatch passes in, so a single instance can serve every workspace.

use std::collections::HashMap;
use std::path::Path;

use agentic_builder::BuilderAppRunner;
use async_trait::async_trait;
use oxy::adapters::workspace::builder::WorkspaceBuilder;
use oxy::exec_types::{Data, DataContainer};
use serde_json::{Value, json};

use crate::server::service::app::AppService;

const MAX_SAMPLE_ROWS: usize = 10;
const MAX_LIST_ITEMS: usize = 20;

/// Runs `*.app.yml` files end-to-end via `AppService::run` and returns a
/// JSON summary of the executed tasks.  Errors propagate verbatim so the
/// builder agent can read the dialect-specific message and propose a fix.
pub struct OxyBuilderAppRunner;

#[async_trait]
impl BuilderAppRunner for OxyBuilderAppRunner {
    async fn run_app(
        &self,
        workspace_root: &Path,
        app_file: &str,
        params: HashMap<String, Value>,
    ) -> Result<Value, String> {
        let workspace_manager = WorkspaceBuilder::new(uuid::Uuid::new_v4())
            .with_working_copy(workspace_root, None, oxy::config::OnMissing::Fail)
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

/// Summarize the executed app's output container.
///
/// At the top level, `AppService::run` returns a `DataContainer::Map`
/// keyed by task name.  We surface a `tasks` array (per-task summary) plus
/// `tasks_run` / `tasks_succeeded` / `tasks_failed` aggregate counts so
/// the builder solver can render a meaningful "X tasks, Y succeeded, Z
/// failed" line.  `WorkflowLauncher::launch_tasks` is fail-fast: if any
/// task errors the entire run returns `Err` (handled in `run_app` above),
/// so reaching this function means every task at least produced output —
/// the success / failure breakdown reflects whether each task's data
/// summary reports `status: "ok"` (typical for tables / text / bool) vs
/// `status: "no_data"` (an empty `Data::None` result).
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
            let tasks_run = tasks.len() as u64;
            let tasks_succeeded = tasks
                .iter()
                .filter(|t| t["result"]["status"].as_str() == Some("ok"))
                .count() as u64;
            let tasks_failed = tasks_run - tasks_succeeded;
            json!({
                "tasks": tasks,
                "tasks_run": tasks_run,
                "tasks_succeeded": tasks_succeeded,
                "tasks_failed": tasks_failed,
            })
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
            if let Some(json_str) = table_data.json.as_deref()
                && let Ok(Value::Array(rows)) = serde_json::from_str::<Value>(json_str)
            {
                let total_rows = rows.len();
                let sample: Vec<Value> = rows.into_iter().take(MAX_SAMPLE_ROWS).collect();
                return json!({
                    "status": "ok",
                    "total_rows": total_rows,
                    "sample_rows": sample,
                });
            }
            json!({ "status": "ok", "note": "table written to parquet (no inline sample)" })
        }
        Data::Text(text) => json!({ "status": "ok", "text": text }),
        Data::Bool(b) => json!({ "status": "ok", "value": b }),
        Data::None => json!({ "status": "no_data" }),
    }
}
