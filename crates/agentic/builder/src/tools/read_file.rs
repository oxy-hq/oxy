use std::path::Path;

use agentic_core::tools::{ToolDef, ToolError};
use serde_json::{Value, json};

use super::utils::{MAX_FILE_CHARS, MAX_FILE_LINES, safe_path};

pub fn read_file_def() -> ToolDef {
    ToolDef {
        name: "read_file",
        description: "Read the content of a file. Returns raw content with no line number \
            formatting. Capped at 500 lines or 100 000 characters, whichever comes first. \
            Use offset/limit to page through large files.",
        parameters: json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file, relative to the project root"
                },
                "offset": {
                    "type": ["integer", "null"],
                    "description": "First line to read (1-indexed, inclusive). Null to start from the beginning."
                },
                "limit": {
                    "type": ["integer", "null"],
                    "description": "Maximum number of lines to return. Null to read up to the 500-line cap."
                }
            },
            "required": ["file_path", "offset", "limit"],
            "additionalProperties": false
        }),
        strict: false,
        ..Default::default()
    }
}

pub async fn execute_read_file(workspace_root: &Path, params: &Value) -> Result<Value, ToolError> {
    let path_str = params["file_path"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'file_path'".into()))?;

    let abs = safe_path(workspace_root, path_str)?;

    let content = tokio::fs::read_to_string(&abs)
        .await
        .map_err(|e| ToolError::Execution(format!("failed to read file '{path_str}': {e}")))?;

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    let start = params["offset"]
        .as_u64()
        .map(|n| (n as usize).saturating_sub(1))
        .unwrap_or(0);

    let limit = params["limit"]
        .as_u64()
        .map(|n| n as usize)
        .unwrap_or(MAX_FILE_LINES)
        .min(MAX_FILE_LINES);

    let end = (start + limit).min(total_lines);

    let selected = &lines[start.min(total_lines)..end];
    let mut char_budget = MAX_FILE_CHARS;
    let mut actual_end = start;
    let mut output_lines: Vec<&str> = Vec::with_capacity(selected.len());
    for (i, line) in selected.iter().enumerate() {
        let cost = line.len() + 1;
        if cost > char_budget && !output_lines.is_empty() {
            break;
        }
        char_budget = char_budget.saturating_sub(cost);
        actual_end = start + i + 1;
        output_lines.push(line);
    }
    let result_content = output_lines.join("\n");

    Ok(json!({
        "content": result_content,
        "total_lines": total_lines,
        "start_line": start + 1,
        "end_line": actual_end,
        "truncated": actual_end < total_lines
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("read_file_test_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        dir.canonicalize().unwrap()
    }

    #[tokio::test]
    async fn raw_content_no_line_numbers() {
        let dir = test_dir().await;
        tokio::fs::write(dir.join("f.txt"), "hello\nworld\n")
            .await
            .unwrap();
        let params = json!({ "file_path": "f.txt", "offset": null, "limit": null });
        let result = execute_read_file(&dir, &params).await.unwrap();
        let content = result["content"].as_str().unwrap();
        assert_eq!(content, "hello\nworld", "got: {content}");
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn char_budget_truncates_long_lines() {
        let dir = test_dir().await;
        // Each line is 1000 chars; 200 lines = 200 000 chars, well above MAX_FILE_CHARS (100 000)
        let long_line = "x".repeat(1000);
        let text: String = (0..200).map(|_| format!("{long_line}\n")).collect();
        tokio::fs::write(dir.join("big.txt"), &text).await.unwrap();
        let params = json!({ "file_path": "big.txt", "offset": null, "limit": null });
        let result = execute_read_file(&dir, &params).await.unwrap();
        assert_eq!(result["total_lines"], 200);
        assert!(result["truncated"].as_bool().unwrap());
        let end_line = result["end_line"].as_u64().unwrap();
        assert!(
            end_line < 200,
            "expected early truncation, got end_line={end_line}"
        );
        let content = result["content"].as_str().unwrap();
        assert!(
            content.len() <= super::MAX_FILE_CHARS + 1000,
            "content too large: {}",
            content.len()
        );
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn offset_and_limit_respected() {
        let dir = test_dir().await;
        tokio::fs::write(dir.join("f.txt"), "a\nb\nc\nd\ne\n")
            .await
            .unwrap();
        let params = json!({ "file_path": "f.txt", "offset": 2, "limit": 2 });
        let result = execute_read_file(&dir, &params).await.unwrap();
        let content = result["content"].as_str().unwrap();
        assert_eq!(content, "b\nc", "got: {content}");
        tokio::fs::remove_dir_all(&dir).await.ok();
    }
}
