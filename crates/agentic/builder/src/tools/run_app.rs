use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use agentic_core::tools::{ToolDef, ToolError};
use serde_json::{Value, json};

use crate::app_runner::BuilderAppRunner;

use super::utils::safe_path;

pub fn run_app_def() -> ToolDef {
    ToolDef {
        name: "run_app",
        description: "Execute a .app.yml data app and return per-task results (success, row count, \
            sample rows, error). Always runs fresh — bypasses the result cache. Use after editing \
            an app file to verify all tasks execute without error.",
        parameters: json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to a .app.yml file, relative to the project root (e.g. 'examples/sales.app.yml')."
                },
                "params_json": {
                    "type": "string",
                    "description": "Optional JSON object string of control parameter values to inject (e.g. '{\"start_date\":\"2024-01-01\"}'). Omit or pass '{}' to use control defaults."
                }
            },
            "required": ["file_path", "params_json"],
            "additionalProperties": false
        }),
        ..Default::default()
    }
}

pub async fn execute_run_app(
    workspace_root: &Path,
    params: &Value,
    app_runner: Arc<dyn BuilderAppRunner>,
) -> Result<Value, ToolError> {
    let file_path = params["file_path"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("file_path is required".into()))?;

    if !file_path.ends_with(".app.yml") {
        return Err(ToolError::BadParams(format!(
            "expected a .app.yml file, got: {file_path}"
        )));
    }

    // Validate path is within project root (sandbox check).
    let _abs = safe_path(workspace_root, file_path)?;

    // Parse optional control params from a JSON string.
    let control_params: HashMap<String, serde_json::Value> = params["params_json"]
        .as_str()
        .filter(|s| !s.is_empty() && *s != "{}")
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| {
            v.as_object()
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        })
        .unwrap_or_default();

    app_runner
        .run_app(workspace_root, file_path, control_params)
        .await
        .map_err(|e| ToolError::Execution(format!("app run failed: {e}")))
}
