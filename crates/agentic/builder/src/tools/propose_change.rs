use std::path::Path;

use agentic_core::{
    human_input::HumanInputProvider,
    tools::{ToolDef, ToolError},
};
use serde_json::{Value, json};

use super::utils::safe_path;

/// A single line-range replacement block.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ChangeBlock {
    /// First line to replace, 1-indexed inclusive.
    pub from_line: usize,
    /// Last line to replace, 1-indexed inclusive.
    pub to_line: usize,
    /// New content that replaces the specified line range.
    pub content: String,
}

pub fn propose_change_def() -> ToolDef {
    ToolDef {
        name: "propose_change",
        description: "Propose a change to a file and ask the user for confirmation before applying it. The user will see a diff and can accept or reject. Provide changes as targeted line-range blocks (from_line..=to_line replaced with content). Set delete=true to propose deleting a file instead.",
        parameters: json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file to change or delete, relative to the project root"
                },
                "changes": {
                    "type": ["array", "null"],
                    "description": "Line-range blocks to apply. Each block: {from_line: integer (1-indexed inclusive), to_line: integer (1-indexed inclusive), content: string}. Null when delete=true.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "from_line": { "type": "integer" },
                            "to_line": { "type": "integer" },
                            "content": { "type": "string" }
                        },
                        "required": ["from_line", "to_line", "content"]
                    }
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
            "required": ["file_path", "description", "changes", "delete"]
        }),
        // `changes` and `delete` use union types (["array","null"], ["boolean","null"])
        // which are incompatible with strict structured-output validation.
        strict: false,
        ..Default::default()
    }
}

/// Apply a list of `ChangeBlock`s to `content`, returning the modified string.
///
/// Blocks are applied from bottom to top (highest `from_line` first) so that
/// earlier block indices remain valid after each splice.
pub fn apply_blocks_to_content(content: &str, blocks: &[ChangeBlock]) -> Result<String, String> {
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();

    // Safety net for the "create from empty" pattern applied to a non-empty file.
    // A single block with from_line=1, to_line=1 and multi-line content replaces
    // only line 1, leaving lines 2..N intact — the new content gets prepended and
    // the file doubles. This is almost always an LLM drafting mistake (it thought
    // the file was empty). Reject with guidance instead of silently duplicating.
    if blocks.len() == 1 {
        let block = &blocks[0];
        let new_line_count = block.content.lines().count();
        let old_line_count = lines.len();
        if block.from_line == 1 && block.to_line == 1 && new_line_count > 1 && old_line_count >= 2 {
            return Err(format!(
                "single block with from_line=1, to_line=1 and {new_line_count}-line content would \
                 leave lines 2..={old_line_count} of the existing file intact and duplicate content. \
                 If you intended to replace the entire file, set to_line={old_line_count}. \
                 If you intended to edit only line 1, keep the new content to a single line."
            ));
        }
    }

    let mut sorted: Vec<&ChangeBlock> = blocks.iter().collect();
    sorted.sort_by(|a, b| b.from_line.cmp(&a.from_line));

    // After sorting descending, adjacent pairs are [higher, lower].  An overlap
    // occurs when the lower block's to_line reaches into the higher block's range.
    for window in sorted.windows(2) {
        let higher = window[0]; // larger from_line
        let lower = window[1]; // smaller from_line
        if lower.to_line >= higher.from_line {
            return Err(format!(
                "blocks overlap: [{},{}] and [{},{}]",
                lower.from_line, lower.to_line, higher.from_line, higher.to_line
            ));
        }
    }

    for block in sorted {
        if block.from_line == 0 {
            return Err(format!("from_line must be >= 1, got {}", block.from_line));
        }
        if block.to_line < block.from_line {
            return Err(format!(
                "to_line ({}) must be >= from_line ({})",
                block.to_line, block.from_line
            ));
        }
        let from = block.from_line - 1; // 0-indexed
        let to = block.to_line.min(lines.len()); // exclusive end, clamped
        if from > lines.len() {
            return Err(format!(
                "from_line {} exceeds file length {}",
                block.from_line,
                lines.len()
            ));
        }
        let replacement: Vec<String> = block.content.lines().map(|l| l.to_string()).collect();
        lines.splice(from..to, replacement);
    }

    let mut result = lines.join("\n");
    // Preserve trailing newline if the original had one.
    if content.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }
    Ok(result)
}

/// Ask the user for confirmation of a proposed file change.
///
/// Uses the [`HumanInputProvider`] to either get an immediate answer (CLI) or
/// suspend the pipeline (`ToolError::Suspended`) for the HTTP layer to handle.
/// The prompt encodes the change metadata as JSON so the frontend can render a diff.
pub async fn execute_propose_change(
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
    let delete = params["delete"].as_bool().unwrap_or(false);

    // Validate path is safe (even for new files).
    let abs = safe_path(workspace_root, file_path)?;

    if delete {
        if params["changes"].as_array().is_some() {
            return Err(ToolError::BadParams(
                "cannot set changes when delete=true".into(),
            ));
        }
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
            Ok(answer) => {
                // Apply immediately when the provider accepts synchronously
                // (e.g. AutoAcceptInputProvider for delegation children).
                if answer.to_lowercase().contains("accept") {
                    delete_file(workspace_root, file_path)
                        .await
                        .map_err(ToolError::Execution)?;
                }
                Ok(json!({ "answer": answer }))
            }
            Err(()) => Err(ToolError::Suspended {
                prompt,
                suggestions,
            }),
        };
    }

    let blocks: Vec<ChangeBlock> = params["changes"]
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| {
            ToolError::BadParams("'changes' is required when 'delete' is not true".into())
        })
        .and_then(|_| {
            serde_json::from_value(params["changes"].clone())
                .map_err(|e| ToolError::BadParams(format!("invalid 'changes' blocks: {e}")))
        })?;

    let old_content = tokio::fs::read_to_string(&abs).await.unwrap_or_default();
    let new_content = apply_blocks_to_content(&old_content, &blocks)
        .map_err(|e| ToolError::BadParams(format!("invalid change block: {e}")))?;

    let prompt = serde_json::json!({
        "type": "propose_change",
        "file_path": file_path,
        "old_content": old_content,
        "new_content": new_content,
        "changes": params["changes"],
        "description": description
    })
    .to_string();

    let suggestions = vec!["Accept".to_string(), "Reject".to_string()];
    match provider.request_sync(&prompt, &suggestions) {
        Ok(answer) => {
            // Apply immediately when the provider accepts synchronously
            // (e.g. AutoAcceptInputProvider for delegation children).
            if answer.to_lowercase().contains("accept") {
                write_file_content(workspace_root, file_path, &new_content)
                    .await
                    .map_err(ToolError::Execution)?;
            }
            Ok(json!({ "answer": answer }))
        }
        Err(()) => Err(ToolError::Suspended {
            prompt,
            suggestions,
        }),
    }
}

/// Write `content` to `file_path` (relative to `workspace_root`), creating
/// parent directories as needed.  Used to persist a pre-computed file state
/// without re-reading or re-applying change blocks.
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

/// Apply previously-proposed block changes to a file (called after user accepts).
///
/// Prefer [`write_file_content`] with a pre-computed `new_content` string when
/// available — reading the file again here creates a TOCTOU window.
pub async fn apply_change_blocks(
    workspace_root: &Path,
    file_path: &str,
    blocks: &[ChangeBlock],
) -> Result<(), String> {
    let abs = safe_path(workspace_root, file_path).map_err(|e| format!("path error: {e}"))?;
    let old_content = tokio::fs::read_to_string(&abs).await.unwrap_or_default();
    let new_content = apply_blocks_to_content(&old_content, blocks)?;
    write_file_content(workspace_root, file_path, &new_content).await
}

/// Delete a file (called after user accepts a deletion proposal).
pub async fn delete_file(workspace_root: &Path, file_path: &str) -> Result<(), String> {
    let abs = safe_path(workspace_root, file_path).map_err(|e| format!("path error: {e}"))?;
    tokio::fs::remove_file(&abs)
        .await
        .map_err(|e| format!("failed to delete file '{file_path}': {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(from_line: usize, to_line: usize, content: &str) -> ChangeBlock {
        ChangeBlock {
            from_line,
            to_line,
            content: content.to_string(),
        }
    }

    #[test]
    fn empty_file_single_insert() {
        let result = apply_blocks_to_content("", &[block(1, 1, "hello")]).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn multiple_non_overlapping_blocks() {
        let content = "line1\nline2\nline3\nline4\nline5\n";
        // Replace line 2 and line 4 independently.
        let result =
            apply_blocks_to_content(content, &[block(2, 2, "TWO"), block(4, 4, "FOUR")]).unwrap();
        assert_eq!(result, "line1\nTWO\nline3\nFOUR\nline5\n");
    }

    #[test]
    fn append_block_at_end() {
        let content = "line1\nline2\n";
        // from_line = file_length + 1 appends after the last line.
        let result = apply_blocks_to_content(content, &[block(3, 3, "line3")]).unwrap();
        assert_eq!(result, "line1\nline2\nline3\n");
    }

    #[test]
    fn trailing_newline_preserved() {
        let content = "line1\nline2\n";
        let result = apply_blocks_to_content(content, &[block(1, 1, "replaced")]).unwrap();
        assert!(
            result.ends_with('\n'),
            "trailing newline should be preserved"
        );
    }

    #[test]
    fn no_trailing_newline_not_added() {
        let content = "line1\nline2";
        let result = apply_blocks_to_content(content, &[block(1, 1, "replaced")]).unwrap();
        assert!(
            !result.ends_with('\n'),
            "trailing newline should not be added"
        );
    }

    #[test]
    fn overlapping_blocks_returns_err() {
        let content = "line1\nline2\nline3\n";
        let err =
            apply_blocks_to_content(content, &[block(1, 3, "a"), block(2, 4, "b")]).unwrap_err();
        assert!(
            err.contains("overlap"),
            "error should mention overlap: {err}"
        );
    }

    #[test]
    fn from_line_exceeds_file_length_returns_err() {
        let content = "line1\nline2\n";
        let err = apply_blocks_to_content(content, &[block(10, 10, "x")]).unwrap_err();
        assert!(
            err.contains("exceeds file length"),
            "error should mention file length: {err}"
        );
    }

    // Regression guard for the onboarding bug where the builder LLM re-drafts a
    // file it already created and calls propose_change with from_line=1, to_line=1
    // thinking the file is empty. Previously this silently duplicated the file
    // contents. Now it should return a BadParams-style error the LLM can act on.
    #[test]
    fn rejects_create_from_empty_on_populated_file() {
        let content = "line1\nline2\nline3\n";
        let err = apply_blocks_to_content(content, &[block(1, 1, "new1\nnew2\nnew3")]).unwrap_err();
        assert!(
            err.contains("to_line=3"),
            "error should suggest setting to_line to the current file length: {err}"
        );
    }

    // The "create from empty" pattern MUST still work on a genuinely empty file.
    #[test]
    fn create_from_empty_pattern_on_empty_file_still_works() {
        let result = apply_blocks_to_content("", &[block(1, 1, "a\nb\nc")]).unwrap();
        assert_eq!(result, "a\nb\nc");
    }

    // Full-file replace (to_line = file length) must still be accepted.
    #[test]
    fn full_file_replace_accepted_when_to_line_matches_length() {
        let content = "a\nb\nc\n";
        let result = apply_blocks_to_content(content, &[block(1, 3, "X\nY")]).unwrap();
        assert_eq!(result, "X\nY\n");
    }

    // Replacing a single line with a single new line must still be accepted.
    #[test]
    fn single_line_edit_on_populated_file_accepted() {
        let content = "a\nb\nc\n";
        let result = apply_blocks_to_content(content, &[block(1, 1, "A")]).unwrap();
        assert_eq!(result, "A\nb\nc\n");
    }

    use agentic_core::human_input::AutoAcceptInputProvider;

    /// Provider that always returns Ok("Reject").
    struct RejectProvider;
    impl HumanInputProvider for RejectProvider {
        fn request_sync(&self, _prompt: &str, _suggestions: &[String]) -> Result<String, ()> {
            Ok("Reject".to_string())
        }
    }

    /// Create a temp dir that survives macOS symlink canonicalization.
    async fn test_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("propose_test_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        dir.canonicalize().unwrap()
    }

    #[tokio::test]
    async fn propose_change_applies_file_on_immediate_accept() {
        let dir = test_dir().await;

        let params = json!({
            "file_path": "new_metric.view.yml",
            "changes": [{
                "from_line": 1,
                "to_line": 1,
                "content": "name: revenue_per_customer\ntype: custom"
            }],
            "description": "add missing metric",
            "delete": null
        });

        let result = execute_propose_change(&dir, &params, &AutoAcceptInputProvider).await;
        assert!(result.is_ok(), "should succeed: {result:?}");

        // File must exist on disk after immediate accept.
        let content = tokio::fs::read_to_string(dir.join("new_metric.view.yml"))
            .await
            .expect("file should have been written");
        assert_eq!(content, "name: revenue_per_customer\ntype: custom");

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn propose_change_does_not_apply_on_reject() {
        let dir = test_dir().await;

        let params = json!({
            "file_path": "should_not_exist.yml",
            "changes": [{
                "from_line": 1,
                "to_line": 1,
                "content": "content"
            }],
            "description": "test",
            "delete": null
        });

        let result = execute_propose_change(&dir, &params, &RejectProvider).await;
        assert!(result.is_ok());

        // File must NOT exist — rejected changes are not applied.
        assert!(
            !dir.join("should_not_exist.yml").exists(),
            "file should not be written on reject"
        );

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn propose_change_deletes_file_on_immediate_accept() {
        let dir = test_dir().await;
        tokio::fs::write(dir.join("to_delete.yml"), "old content")
            .await
            .unwrap();

        let params = json!({
            "file_path": "to_delete.yml",
            "new_content": null,
            "description": "remove obsolete file",
            "delete": true
        });

        let result = execute_propose_change(&dir, &params, &AutoAcceptInputProvider).await;
        assert!(result.is_ok());

        // File must be gone after accepted deletion.
        assert!(
            !dir.join("to_delete.yml").exists(),
            "file should have been deleted"
        );

        tokio::fs::remove_dir_all(&dir).await.ok();
    }
}
