use std::path::Path;

use agentic_core::tools::{ToolDef, ToolError};
use serde_json::{Value, json};

use super::utils::MAX_SEARCH_RESULTS;

pub fn search_text_def() -> ToolDef {
    ToolDef {
        name: "search_text",
        description: "Search for text (regex or literal) across project files. \
            output_mode controls format: \
            'content' (default) returns file:line:text per match; \
            'files_with_matches' returns unique file paths; \
            'count' returns the total match count.",
        parameters: json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Text to search for. Can be a regex pattern or a literal string."
                },
                "glob": {
                    "type": ["string", "null"],
                    "description": "Glob pattern to restrict which files are searched, \
                        e.g. '**/*.yml'. Null to search all files."
                },
                "output_mode": {
                    "type": ["string", "null"],
                    "description": "'content' (default) — file:line:text per match; \
                        'files_with_matches' — unique file paths only; \
                        'count' — total match count as a number."
                }
            },
            "required": ["pattern", "glob", "output_mode"],
            "additionalProperties": false
        }),
        strict: false,
        ..Default::default()
    }
}

pub async fn execute_search_text(
    workspace_root: &Path,
    params: &Value,
) -> Result<Value, ToolError> {
    let pattern_str = params["pattern"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'pattern'".into()))?;

    let file_glob = params["glob"].as_str().unwrap_or("**/*");
    let output_mode = params["output_mode"].as_str().unwrap_or("content");

    let re = regex::RegexBuilder::new(pattern_str)
        .size_limit(1 << 20)
        .dfa_size_limit(1 << 20)
        .build()
        .map_err(|e| ToolError::BadParams(format!("invalid regex pattern: {e}")))?;

    let glob_pattern = workspace_root.join(file_glob);
    let glob_str = glob_pattern
        .to_str()
        .ok_or_else(|| ToolError::BadParams("invalid glob encoding".into()))?;

    let paths =
        glob::glob(glob_str).map_err(|e| ToolError::BadParams(format!("invalid glob: {e}")))?;

    let mut result_lines: Vec<String> = Vec::new();
    let mut matched_files: Vec<String> = Vec::new();
    let mut total_count: usize = 0;
    let mut truncated = false;

    'outer: for entry in paths.flatten() {
        if !entry.is_file() {
            continue;
        }
        let Ok(content) = tokio::fs::read_to_string(&entry).await else {
            continue;
        };
        let rel = entry
            .strip_prefix(workspace_root)
            .unwrap_or(&entry)
            .to_string_lossy()
            .to_string();

        let mut file_matched = false;
        for (line_no, line) in content.lines().enumerate() {
            if re.is_match(line) {
                total_count += 1;
                if !file_matched && output_mode == "files_with_matches" {
                    matched_files.push(rel.clone());
                }
                file_matched = true;
                if output_mode == "content" {
                    result_lines.push(format!("{}:{}:{}", rel, line_no + 1, line));
                }
                if total_count >= MAX_SEARCH_RESULTS {
                    truncated = true;
                    break 'outer;
                }
            }
        }
    }

    let result_str = match output_mode {
        "files_with_matches" => matched_files.join("\n"),
        "count" => total_count.to_string(),
        _ => result_lines.join("\n"),
    };

    Ok(json!({
        "result": result_str,
        "count": total_count,
        "truncated": truncated
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("search_text_test_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        dir.canonicalize().unwrap()
    }

    #[tokio::test]
    async fn content_mode_returns_grep_format() {
        let dir = test_dir().await;
        tokio::fs::write(dir.join("a.yml"), "name: foo\nmodel: gpt-4o\n")
            .await
            .unwrap();
        let params = json!({ "pattern": "name:", "glob": "*.yml", "output_mode": "content" });
        let result = execute_search_text(&dir, &params).await.unwrap();
        let text = result["result"].as_str().unwrap();
        assert!(text.contains("a.yml:1:name: foo"), "got: {text}");
        assert_eq!(result["count"].as_u64().unwrap(), 1);
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn files_with_matches_mode() {
        let dir = test_dir().await;
        tokio::fs::write(dir.join("a.yml"), "execute_sql: true\n")
            .await
            .unwrap();
        tokio::fs::write(dir.join("b.yml"), "no match here\n")
            .await
            .unwrap();
        let params = json!({ "pattern": "execute_sql", "glob": "*.yml", "output_mode": "files_with_matches" });
        let result = execute_search_text(&dir, &params).await.unwrap();
        let text = result["result"].as_str().unwrap();
        assert!(text.contains("a.yml"), "got: {text}");
        assert!(!text.contains("b.yml"), "got: {text}");
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn count_mode() {
        let dir = test_dir().await;
        tokio::fs::write(dir.join("a.yml"), "foo\nfoo\nbar\n")
            .await
            .unwrap();
        let params = json!({ "pattern": "foo", "glob": "*.yml", "output_mode": "count" });
        let result = execute_search_text(&dir, &params).await.unwrap();
        assert_eq!(result["result"].as_str().unwrap(), "2");
        assert_eq!(result["count"].as_u64().unwrap(), 2);
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn default_mode_is_content() {
        let dir = test_dir().await;
        tokio::fs::write(dir.join("a.yml"), "hello world\n")
            .await
            .unwrap();
        let params = json!({ "pattern": "hello", "glob": null });
        let result = execute_search_text(&dir, &params).await.unwrap();
        let text = result["result"].as_str().unwrap();
        assert!(text.contains("hello world"), "got: {text}");
        tokio::fs::remove_dir_all(&dir).await.ok();
    }
}
