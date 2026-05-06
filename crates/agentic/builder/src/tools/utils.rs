use std::path::{Path, PathBuf};

use agentic_core::{human_input::HumanInputProvider, tools::ToolError};
use serde_json::{Value, json};

// ── Constants ────────────────────────────────────────────────────────────────

pub const MAX_FILE_LINES: usize = 500;
pub const MAX_FILE_CHARS: usize = 100_000;
pub const MAX_SEARCH_RESULTS: usize = 50;
pub const MAX_FILE_RESULTS: usize = 100;

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Validate that `path` is within `workspace_root`. Returns the resolved absolute path.
pub fn safe_path(workspace_root: &Path, path: &str) -> Result<PathBuf, ToolError> {
    // Reject obviously dangerous paths before canonicalize (which requires file to exist).
    if path.contains("..") {
        return Err(ToolError::BadParams(format!(
            "path traversal not allowed: {path}"
        )));
    }

    let joined = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        workspace_root.join(path)
    };

    // Check prefix before canonicalize so we can give a clear error.
    let canonical_root = workspace_root
        .canonicalize()
        .map_err(|e| ToolError::Execution(format!("failed to resolve project root: {e}")))?;

    // Resolve symlinks only if the file exists; otherwise just check the path prefix.
    let abs = if joined.exists() {
        joined
            .canonicalize()
            .map_err(|e| ToolError::Execution(format!("failed to resolve path: {e}")))?
    } else {
        // For non-existent files (e.g. file_change creating a new file),
        // normalize the path without canonicalize.
        let mut components = Vec::new();
        for c in joined.components() {
            use std::path::Component;
            match c {
                Component::ParentDir => {
                    components.pop();
                }
                Component::CurDir => {}
                other => components.push(other),
            }
        }
        components.iter().collect::<PathBuf>()
    };

    if !abs.starts_with(&canonical_root) {
        return Err(ToolError::BadParams(format!(
            "path is outside project root: {path}"
        )));
    }

    Ok(abs)
}

// ── HITL and I/O helpers ────────────────────────────────────────────────────

/// Specifies what disk action to perform when the user accepts a HITL prompt.
pub enum ApplyAction<'a> {
    WriteContent { content: &'a str },
    DeleteFile,
}

/// Send an Accept/Reject HITL prompt. Returns the user's answer or suspends.
pub fn hitl_confirm(
    provider: &dyn HumanInputProvider,
    prompt: String,
) -> Result<String, ToolError> {
    let suggestions = vec!["Accept".to_string(), "Reject".to_string()];
    match provider.request_sync(&prompt, &suggestions) {
        Ok(answer) => Ok(answer),
        Err(()) => Err(ToolError::Suspended {
            prompt,
            suggestions,
        }),
    }
}

/// Request user confirmation for a file operation via HITL, then apply the action if accepted.
pub async fn suspend_or_apply(
    workspace_root: &Path,
    provider: &dyn HumanInputProvider,
    prompt_type: &str,
    file_path: &str,
    old_content: &str,
    new_content: &str,
    description: &str,
    action: ApplyAction<'_>,
) -> Result<Value, ToolError> {
    let prompt = json!({
        "type": prompt_type,
        "file_path": file_path,
        "old_content": old_content,
        "new_content": new_content,
        "description": description,
    })
    .to_string();
    let answer = hitl_confirm(provider, prompt)?;
    if answer.to_lowercase().contains("accept") {
        match action {
            ApplyAction::WriteContent { content } => {
                write_file_content(workspace_root, file_path, content)
                    .await
                    .map_err(ToolError::Execution)?;
            }
            ApplyAction::DeleteFile => {
                remove_file(workspace_root, file_path)
                    .await
                    .map_err(ToolError::Execution)?;
            }
        }
    }
    Ok(json!({ "answer": answer }))
}

/// Write content to a file, creating parent directories as needed.
pub async fn write_file_content(
    workspace_root: &Path,
    file_path: &str,
    content: &str,
) -> Result<(), String> {
    let abs = safe_path(workspace_root, file_path).map_err(|e| format!("path error: {e}"))?;
    if let Some(parent) = abs.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed to create directories: {e}"))?;
    }
    tokio::fs::write(&abs, content)
        .await
        .map_err(|e| format!("failed to write file '{file_path}': {e}"))
}

/// Delete a file at the given path.
pub async fn remove_file(workspace_root: &Path, file_path: &str) -> Result<(), String> {
    let abs = safe_path(workspace_root, file_path).map_err(|e| format!("path error: {e}"))?;
    tokio::fs::remove_file(&abs)
        .await
        .map_err(|e| format!("failed to delete file '{file_path}': {e}"))
}
