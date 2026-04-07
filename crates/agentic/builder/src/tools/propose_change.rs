use std::path::Path;

use agentic_core::{
    human_input::HumanInputProvider,
    tools::{ToolDef, ToolError},
};
use serde_json::{json, Value};

use super::utils::safe_path;

pub fn propose_change_def() -> ToolDef {
    ToolDef {
        name: "propose_change",
        description: "Propose a change to a file and ask the user for confirmation before applying it. The user will see a diff and can accept or reject. Set delete=true to propose deleting a file instead of modifying it.",
        parameters: json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file to change or delete, relative to the project root"
                },
                "new_content": {
                    "type": ["string", "null"],
                    "description": "The complete new content of the file after the change. Null when delete=true."
                },
                "description": {
                    "type": "string",
                    "description": "Human-readable description of what this change does and why"
                },
                "delete": {
                    "type": ["boolean", "null"],
                    "description": "Set to true to propose deleting the file instead of modifying it. Null defaults to false."
                }
            },
            "required": ["file_path", "description", "new_content", "delete"],
            "additionalProperties": false
        }),
    }
}

/// Ask the user for confirmation of a proposed file change.
///
/// Uses the [`HumanInputProvider`] to either get an immediate answer (CLI) or
/// suspend the pipeline (`ToolError::Suspended`) for the HTTP layer to handle.
/// The prompt encodes the change metadata as JSON so the frontend can render a diff.
pub async fn execute_propose_change(
    project_root: &Path,
    params: &Value,
    provider: &dyn HumanInputProvider,
) -> Result<Value, ToolError> {
    let file_path = params["file_path"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'file_path'".into()))?;
    let description = params["description"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'description'".into()))?;
    let delete = params["delete"].as_bool().unwrap_or(false);

    if delete && params["new_content"].is_string() {
        return Err(ToolError::BadParams(
            "cannot set new_content when delete=true".into(),
        ));
    }

    // Validate path is safe (even for new files).
    let abs = safe_path(project_root, file_path)?;

    if delete {
        // For deletion, the file must exist.
        if !abs.exists() {
            return Err(ToolError::BadParams(format!(
                "file does not exist: {file_path}"
            )));
        }
        let old_content = tokio::fs::read_to_string(&abs)
            .await
            .map_err(|e| ToolError::Execution(format!("failed to read file '{file_path}': {e}")))?;

        let prompt = serde_json::json!({
            "type": "propose_change",
            "file_path": file_path,
            "old_content": old_content,
            "new_content": "",
            "description": description,
            "delete": true
        })
        .to_string();

        let suggestions = vec!["Accept".to_string(), "Reject".to_string()];
        return match provider.request_sync(&prompt, &suggestions) {
            Ok(answer) => Ok(json!({ "answer": answer })),
            Err(()) => Err(ToolError::Suspended {
                prompt,
                suggestions,
            }),
        };
    }

    let new_content = params["new_content"].as_str().ok_or_else(|| {
        ToolError::BadParams("missing 'new_content' (required when delete is not true)".into())
    })?;

    // Read the existing file content (empty string for new files).
    let old_content = tokio::fs::read_to_string(&abs).await.unwrap_or_default();

    // Encode the change as JSON in the prompt so the frontend can render a diff.
    let prompt = serde_json::json!({
        "type": "propose_change",
        "file_path": file_path,
        "old_content": old_content,
        "new_content": new_content,
        "description": description
    })
    .to_string();

    let suggestions = vec!["Accept".to_string(), "Reject".to_string()];
    match provider.request_sync(&prompt, &suggestions) {
        Ok(answer) => Ok(json!({ "answer": answer })),
        Err(()) => Err(ToolError::Suspended {
            prompt,
            suggestions,
        }),
    }
}

/// Apply a previously-proposed change (called after user accepts).
pub async fn apply_change(
    project_root: &Path,
    file_path: &str,
    new_content: &str,
) -> Result<(), String> {
    let abs = safe_path(project_root, file_path).map_err(|e| format!("path error: {e}"))?;

    // Create parent directories if needed.
    if let Some(parent) = abs.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed to create directories: {e}"))?;
    }

    tokio::fs::write(&abs, new_content)
        .await
        .map_err(|e| format!("failed to write file '{file_path}': {e}"))
}

/// Delete a file (called after user accepts a deletion proposal).
pub async fn delete_file(project_root: &Path, file_path: &str) -> Result<(), String> {
    let abs = safe_path(project_root, file_path).map_err(|e| format!("path error: {e}"))?;
    tokio::fs::remove_file(&abs)
        .await
        .map_err(|e| format!("failed to delete file '{file_path}': {e}"))
}
