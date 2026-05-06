use std::path::Path;

use agentic_core::{
    human_input::HumanInputProvider,
    tools::{ToolDef, ToolError},
};
use serde_json::{Value, json};

use super::utils::{ApplyAction, safe_path, suspend_or_apply};

pub fn edit_file_def() -> ToolDef {
    ToolDef {
        name: "edit_file",
        description: "Replace an exact string in an existing file. \
            old_string must match character-for-character including whitespace and indentation. \
            Fails if old_string is not found. Prefer this over write_file for targeted edits. \
            Set replace_all=true to replace every occurrence. \
            The user will see a diff and can accept or reject.",
        parameters: json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file, relative to the project root"
                },
                "old_string": {
                    "type": "string",
                    "description": "Exact string to find and replace, including all whitespace and indentation"
                },
                "new_string": {
                    "type": "string",
                    "description": "Replacement text"
                },
                "description": {
                    "type": "string",
                    "description": "Human-readable description of what this change does and why"
                },
                "replace_all": {
                    "type": ["boolean", "null"],
                    "description": "Replace all occurrences. Defaults to false (first occurrence only)."
                }
            },
            "required": ["file_path", "old_string", "new_string", "description", "replace_all"],
            "additionalProperties": false
        }),
        strict: false,
        ..Default::default()
    }
}

/// Apply exact-string replacement to `content`.
/// Returns `Err` if `old_string` is empty or not found.
pub fn apply_edit(
    content: &str,
    old_string: &str,
    new_string: &str,
    replace_all: bool,
) -> Result<String, String> {
    if old_string.is_empty() {
        return Err("old_string must not be empty".into());
    }
    if !content.contains(old_string) {
        return Err("old_string not found in file".into());
    }
    if replace_all {
        Ok(content.replace(old_string, new_string))
    } else {
        Ok(content.replacen(old_string, new_string, 1))
    }
}

pub async fn execute_edit_file(
    workspace_root: &Path,
    params: &Value,
    provider: &dyn HumanInputProvider,
) -> Result<Value, ToolError> {
    let file_path = params["file_path"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'file_path'".into()))?;
    let old_string = params["old_string"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'old_string'".into()))?;
    let new_string = params["new_string"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'new_string'".into()))?;
    let description = params["description"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'description'".into()))?;
    let replace_all = params["replace_all"].as_bool().unwrap_or(false);

    let abs = safe_path(workspace_root, file_path)?;
    if !abs.exists() {
        return Err(ToolError::BadParams(format!(
            "file does not exist: {file_path}"
        )));
    }

    let old_content = tokio::fs::read_to_string(&abs)
        .await
        .map_err(|e| ToolError::Execution(format!("failed to read '{file_path}': {e}")))?;

    let new_content = apply_edit(&old_content, old_string, new_string, replace_all)
        .map_err(ToolError::BadParams)?;

    suspend_or_apply(
        workspace_root,
        provider,
        "edit_file",
        file_path,
        &old_content,
        &new_content,
        description,
        ApplyAction::WriteContent {
            content: &new_content,
        },
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

    // ── apply_edit unit tests ─────────────────────────────────────────────────

    #[test]
    fn replaces_first_occurrence() {
        let result = apply_edit("foo bar foo", "foo", "baz", false).unwrap();
        assert_eq!(result, "baz bar foo");
    }

    #[test]
    fn replace_all_replaces_every_occurrence() {
        let result = apply_edit("foo bar foo", "foo", "baz", true).unwrap();
        assert_eq!(result, "baz bar baz");
    }

    #[test]
    fn not_found_returns_err() {
        let err = apply_edit("hello world", "missing", "x", false).unwrap_err();
        assert!(err.contains("not found"), "unexpected error: {err}");
    }

    #[test]
    fn empty_old_string_returns_err() {
        let err = apply_edit("hello", "", "x", false).unwrap_err();
        assert!(err.contains("empty"), "unexpected error: {err}");
    }

    #[test]
    fn preserves_trailing_newline() {
        let result = apply_edit("line1\nline2\n", "line1", "LINE1", false).unwrap();
        assert_eq!(result, "LINE1\nline2\n");
    }

    #[test]
    fn replace_all_false_with_single_occurrence() {
        let result = apply_edit("only once", "once", "ONCE", false).unwrap();
        assert_eq!(result, "only ONCE");
    }

    // ── execute_edit_file integration tests ───────────────────────────────────

    async fn test_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("edit_file_test_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        dir.canonicalize().unwrap()
    }

    #[tokio::test]
    async fn applies_edit_on_accept() {
        let dir = test_dir().await;
        tokio::fs::write(dir.join("file.yml"), "name: old\nmodel: gpt-4o\n")
            .await
            .unwrap();
        let params = json!({
            "file_path": "file.yml",
            "old_string": "name: old",
            "new_string": "name: new",
            "description": "rename agent",
            "replace_all": null
        });
        execute_edit_file(&dir, &params, &AutoAcceptInputProvider)
            .await
            .unwrap();
        let written = tokio::fs::read_to_string(dir.join("file.yml"))
            .await
            .unwrap();
        assert_eq!(written, "name: new\nmodel: gpt-4o\n");
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn does_not_write_on_reject() {
        let dir = test_dir().await;
        tokio::fs::write(dir.join("file.yml"), "original")
            .await
            .unwrap();
        let params = json!({
            "file_path": "file.yml",
            "old_string": "original",
            "new_string": "changed",
            "description": "test"
        });
        execute_edit_file(&dir, &params, &RejectProvider)
            .await
            .unwrap();
        let content = tokio::fs::read_to_string(dir.join("file.yml"))
            .await
            .unwrap();
        assert_eq!(content, "original");
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn errors_when_file_missing() {
        let dir = test_dir().await;
        let params = json!({
            "file_path": "nonexistent.yml",
            "old_string": "x",
            "new_string": "y",
            "description": "test"
        });
        let err = execute_edit_file(&dir, &params, &AutoAcceptInputProvider)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::BadParams(_)));
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn errors_when_old_string_not_found() {
        let dir = test_dir().await;
        tokio::fs::write(dir.join("file.yml"), "hello world")
            .await
            .unwrap();
        let params = json!({
            "file_path": "file.yml",
            "old_string": "missing",
            "new_string": "x",
            "description": "test"
        });
        let err = execute_edit_file(&dir, &params, &AutoAcceptInputProvider)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::BadParams(_)));
        tokio::fs::remove_dir_all(&dir).await.ok();
    }
}
