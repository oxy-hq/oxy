use std::path::Path;

use agentic_core::{
    human_input::HumanInputProvider,
    tools::{ToolDef, ToolError},
};
use serde_json::{Value, json};

use super::utils::{ApplyAction, safe_path, suspend_or_apply};

pub fn delete_file_def() -> ToolDef {
    ToolDef {
        name: "delete_file",
        description: "Delete an existing file. The user will see the current file content and can accept or reject.",
        parameters: json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file to delete, relative to the project root"
                },
                "description": {
                    "type": "string",
                    "description": "Human-readable description of why this file is being deleted"
                }
            },
            "required": ["file_path", "description"],
            "additionalProperties": false
        }),
        ..Default::default()
    }
}

pub async fn execute_delete_file(
    workspace_root: &Path,
    params: &Value,
    provider: &dyn HumanInputProvider,
) -> Result<Value, ToolError> {
    let file_path = params["file_path"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'file_path'".into()))?;
    let description = params["description"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'description'".into()))?;

    let abs = safe_path(workspace_root, file_path)?;
    if !abs.exists() {
        return Err(ToolError::BadParams(format!(
            "file does not exist: {file_path}"
        )));
    }

    let old_content = tokio::fs::read_to_string(&abs)
        .await
        .map_err(|e| ToolError::Execution(format!("failed to read '{file_path}': {e}")))?;

    suspend_or_apply(
        workspace_root,
        provider,
        "delete_file",
        file_path,
        &old_content,
        "",
        description,
        ApplyAction::DeleteFile,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_core::human_input::AutoAcceptInputProvider;

    struct RejectProvider;
    impl HumanInputProvider for RejectProvider {
        fn request_sync(&self, _prompt: &str, _suggestions: &[String]) -> Result<String, ()> {
            Ok("Reject".to_string())
        }
    }

    async fn test_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("delete_file_test_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        dir.canonicalize().unwrap()
    }

    #[tokio::test]
    async fn deletes_file_on_accept() {
        let dir = test_dir().await;
        tokio::fs::write(dir.join("to_delete.yml"), "old content")
            .await
            .unwrap();
        let params = json!({
            "file_path": "to_delete.yml",
            "description": "remove obsolete file"
        });
        execute_delete_file(&dir, &params, &AutoAcceptInputProvider)
            .await
            .unwrap();
        assert!(!dir.join("to_delete.yml").exists());
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn does_not_delete_on_reject() {
        let dir = test_dir().await;
        tokio::fs::write(dir.join("keep.yml"), "content")
            .await
            .unwrap();
        let params = json!({
            "file_path": "keep.yml",
            "description": "test"
        });
        execute_delete_file(&dir, &params, &RejectProvider)
            .await
            .unwrap();
        assert!(dir.join("keep.yml").exists());
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn errors_when_file_missing() {
        let dir = test_dir().await;
        let params = json!({
            "file_path": "nonexistent.yml",
            "description": "test"
        });
        let err = execute_delete_file(&dir, &params, &AutoAcceptInputProvider)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::BadParams(_)));
        tokio::fs::remove_dir_all(&dir).await.ok();
    }
}
