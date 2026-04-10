use std::path::Path;

use agentic_core::tools::{ToolDef, ToolError};
use serde_json::{Value, json};

use super::utils::MAX_FILE_RESULTS;

pub fn search_files_def() -> ToolDef {
    ToolDef {
        name: "search_files",
        description: "Search for files in the project using a glob pattern (e.g. '**/*.sql', 'agents/*.agent.yml'). Returns matching file paths and sizes.",
        parameters: json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern relative to the project root, e.g. '**/*.yml' or 'src/**/*.rs'"
                }
            },
            "required": ["pattern"],
            "additionalProperties": false
        }),
    }
}

pub fn execute_search_files(workspace_root: &Path, params: &Value) -> Result<Value, ToolError> {
    let pattern = params["pattern"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'pattern'".into()))?;

    let glob_pattern = workspace_root.join(pattern);
    let glob_str = glob_pattern
        .to_str()
        .ok_or_else(|| ToolError::BadParams("invalid pattern encoding".into()))?;

    let paths = glob::glob(glob_str)
        .map_err(|e| ToolError::BadParams(format!("invalid glob pattern: {e}")))?;

    let mut files = Vec::new();
    for entry in paths.flatten().take(MAX_FILE_RESULTS) {
        if entry.is_file() {
            let rel = entry
                .strip_prefix(workspace_root)
                .unwrap_or(&entry)
                .to_string_lossy()
                .to_string();
            let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            files.push(json!({ "path": rel, "size_bytes": size_bytes }));
        }
    }

    Ok(json!({ "files": files, "count": files.len() }))
}
