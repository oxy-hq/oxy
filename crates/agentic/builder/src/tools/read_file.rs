use std::path::Path;

use agentic_core::tools::{ToolDef, ToolError};
use serde_json::{json, Value};

use super::utils::{safe_path, MAX_FILE_LINES};

pub fn read_file_def() -> ToolDef {
    ToolDef {
        name: "read_file",
        description: "Read the content of a file. Optionally specify a line range. Returns the content and total line count.",
        parameters: json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file, relative to the project root"
                },
                "start_line": {
                    "type": ["integer", "null"],
                    "description": "First line to read (1-indexed, inclusive). Null to start from the beginning."
                },
                "end_line": {
                    "type": ["integer", "null"],
                    "description": "Last line to read (1-indexed, inclusive). Null to read to the end (capped at 500 lines)."
                }
            },
            "required": ["path", "start_line", "end_line"],
            "additionalProperties": false
        }),
    }
}

pub async fn execute_read_file(project_root: &Path, params: &Value) -> Result<Value, ToolError> {
    let path_str = params["path"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'path'".into()))?;

    let abs = safe_path(project_root, path_str)?;

    let content = tokio::fs::read_to_string(&abs)
        .await
        .map_err(|e| ToolError::Execution(format!("failed to read file '{path_str}': {e}")))?;

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    let start = params["start_line"]
        .as_u64()
        .map(|n| (n as usize).saturating_sub(1))
        .unwrap_or(0);
    let end = params["end_line"]
        .as_u64()
        .map(|n| (n as usize).min(total_lines))
        .unwrap_or((start + MAX_FILE_LINES).min(total_lines));

    let end = end.min(start + MAX_FILE_LINES);
    let selected: Vec<&str> = lines[start.min(total_lines)..end.min(total_lines)].to_vec();
    let result_content = selected.join("\n");

    Ok(json!({
        "content": result_content,
        "total_lines": total_lines,
        "start_line": start + 1,
        "end_line": end,
        "truncated": end < total_lines && params["end_line"].is_null()
    }))
}
