use std::path::Path;

use agentic_core::{
    human_input::HumanInputProvider,
    tools::{ToolDef, ToolError},
};
use serde_json::{Value, json};

use super::utils::safe_path;

pub fn manage_directory_def() -> ToolDef {
    ToolDef {
        name: "manage_directory",
        description: "Create, delete, or rename a directory within the project. \
                      Requires user confirmation before applying any change. \
                      Use operation=\"create\" to create a directory (and any missing parents). \
                      Use operation=\"delete\" to recursively delete a directory and all its contents. \
                      Use operation=\"rename\" to rename or move a directory (new_path required).",
        parameters: json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["create", "delete", "rename"],
                    "description": "The directory operation to perform."
                },
                "path": {
                    "type": "string",
                    "description": "Path to the directory, relative to the project root."
                },
                "new_path": {
                    "type": ["string", "null"],
                    "description": "Destination path for rename operations, relative to the project root. Required when operation=\"rename\"."
                },
                "description": {
                    "type": "string",
                    "description": "Human-readable explanation of why this directory change is needed."
                }
            },
            "required": ["operation", "path", "description"]
        }),
        strict: false,
        ..Default::default()
    }
}

/// Collect relative paths of all files inside `dir` (up to `limit`).
fn list_directory_contents(dir: &Path, workspace_root: &Path, limit: usize) -> Vec<String> {
    let mut results = Vec::new();
    collect_entries(dir, workspace_root, &mut results, limit, 0);
    results
}

fn collect_entries(
    dir: &Path,
    workspace_root: &Path,
    results: &mut Vec<String>,
    limit: usize,
    depth: usize,
) {
    if results.len() >= limit || depth > 20 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if results.len() >= limit {
            return;
        }
        let path = entry.path();
        let rel = path
            .strip_prefix(workspace_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        if path.is_dir() {
            collect_entries(&path, workspace_root, results, limit, depth + 1);
        } else {
            results.push(rel);
        }
    }
}

pub async fn execute_manage_directory(
    workspace_root: &Path,
    params: &Value,
    provider: &dyn HumanInputProvider,
) -> Result<Value, ToolError> {
    let operation = params["operation"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'operation'".into()))?;
    let path = params["path"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'path'".into()))?;
    let description = params["description"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'description'".into()))?;

    let abs = safe_path(workspace_root, path)?;

    match operation {
        "create" => {
            let prompt = json!({
                "type": "manage_directory",
                "operation": "create",
                "path": path,
                "description": description
            })
            .to_string();
            let suggestions = vec!["Accept".to_string(), "Reject".to_string()];
            match provider.request_sync(&prompt, &suggestions) {
                Ok(answer) => {
                    if answer.to_lowercase().contains("accept") {
                        tokio::fs::create_dir_all(&abs).await.map_err(|e| {
                            ToolError::Execution(format!(
                                "failed to create directory '{path}': {e}"
                            ))
                        })?;
                    }
                    Ok(json!({ "answer": answer }))
                }
                Err(()) => Err(ToolError::Suspended {
                    prompt,
                    suggestions,
                }),
            }
        }
        "delete" => {
            if !abs.exists() {
                return Err(ToolError::BadParams(format!(
                    "directory does not exist: {path}"
                )));
            }
            if !abs.is_dir() {
                return Err(ToolError::BadParams(format!("not a directory: {path}")));
            }
            let contents = list_directory_contents(&abs, workspace_root, 50);
            let prompt = json!({
                "type": "manage_directory",
                "operation": "delete",
                "path": path,
                "description": description,
                "contents_preview": contents
            })
            .to_string();
            let suggestions = vec!["Accept".to_string(), "Reject".to_string()];
            match provider.request_sync(&prompt, &suggestions) {
                Ok(answer) => {
                    if answer.to_lowercase().contains("accept") {
                        tokio::fs::remove_dir_all(&abs).await.map_err(|e| {
                            ToolError::Execution(format!(
                                "failed to delete directory '{path}': {e}"
                            ))
                        })?;
                    }
                    Ok(json!({ "answer": answer }))
                }
                Err(()) => Err(ToolError::Suspended {
                    prompt,
                    suggestions,
                }),
            }
        }
        "rename" => {
            let new_path = params["new_path"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    ToolError::BadParams("'new_path' is required for the rename operation".into())
                })?;
            let abs_new = safe_path(workspace_root, new_path)?;
            if !abs.exists() {
                return Err(ToolError::BadParams(format!(
                    "directory does not exist: {path}"
                )));
            }
            let prompt = json!({
                "type": "manage_directory",
                "operation": "rename",
                "path": path,
                "new_path": new_path,
                "description": description
            })
            .to_string();
            let suggestions = vec!["Accept".to_string(), "Reject".to_string()];
            match provider.request_sync(&prompt, &suggestions) {
                Ok(answer) => {
                    if answer.to_lowercase().contains("accept") {
                        if let Some(parent) = abs_new.parent() {
                            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                                ToolError::Execution(format!(
                                    "failed to create parent directories for '{new_path}': {e}"
                                ))
                            })?;
                        }
                        tokio::fs::rename(&abs, &abs_new).await.map_err(|e| {
                            ToolError::Execution(format!(
                                "failed to rename '{path}' to '{new_path}': {e}"
                            ))
                        })?;
                    }
                    Ok(json!({ "answer": answer }))
                }
                Err(()) => Err(ToolError::Suspended {
                    prompt,
                    suggestions,
                }),
            }
        }
        other => Err(ToolError::BadParams(format!(
            "unknown operation '{other}', expected one of: create, delete, rename"
        ))),
    }
}
