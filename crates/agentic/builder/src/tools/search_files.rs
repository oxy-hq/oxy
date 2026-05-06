use std::path::Path;
use std::time::SystemTime;

use agentic_core::tools::{ToolDef, ToolError};
use serde_json::{Value, json};

use super::utils::MAX_FILE_RESULTS;

pub fn search_files_def() -> ToolDef {
    ToolDef {
        name: "search_files",
        description: "Search for files in the project using a glob pattern \
            (e.g. '**/*.sql', 'agents/*.agent.yml'). \
            Returns matching file paths sorted by modification time (newest first).",
        parameters: json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern relative to the project root. \
                        Use '**/*' to match files recursively \
                        (e.g. 'generated/**/*' for all files under generated/, \
                        'src/**/*.rs' for Rust files). \
                        Use '*' for a single directory level \
                        (e.g. 'agents/*.agent.yml'). \
                        NOTE: a bare trailing '**' like 'generated/**' does NOT match files \
                        — always append '/*' for recursive file search."
                }
            },
            "required": ["pattern"],
            "additionalProperties": false
        }),
        ..Default::default()
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

    let mut entries: Vec<(String, SystemTime)> = Vec::new();
    let mut truncated = false;
    for entry in paths.flatten().filter(|e| e.is_file()) {
        if entries.len() >= MAX_FILE_RESULTS {
            truncated = true;
            break;
        }
        let rel = entry
            .strip_prefix(workspace_root)
            .unwrap_or(&entry)
            .to_string_lossy()
            .to_string();
        let mtime = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        entries.push((rel, mtime));
    }

    // Newest first, matching Claude Code's Glob sort order.
    entries.sort_by(|a, b| b.1.cmp(&a.1));

    let files: Vec<serde_json::Value> = entries
        .into_iter()
        .map(|(path, _)| serde_json::Value::String(path))
        .collect();
    let count = files.len();

    Ok(json!({ "files": files, "count": count, "truncated": truncated }))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("search_files_test_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        dir.canonicalize().unwrap()
    }

    #[tokio::test]
    async fn returns_plain_string_paths() {
        let dir = test_dir().await;
        tokio::fs::write(dir.join("a.yml"), "").await.unwrap();
        tokio::fs::write(dir.join("b.yml"), "").await.unwrap();
        let params = json!({ "pattern": "*.yml" });
        let result = execute_search_files(&dir, &params).unwrap();
        let files = result["files"].as_array().unwrap();
        for f in files {
            assert!(f.is_string(), "expected string, got: {f}");
        }
        assert_eq!(result["count"].as_u64().unwrap(), 2);
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn newest_file_appears_first() {
        let dir = test_dir().await;
        tokio::fs::write(dir.join("older.yml"), "").await.unwrap();
        // Sleep long enough for mtime granularity (1s on most filesystems).
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        tokio::fs::write(dir.join("newer.yml"), "").await.unwrap();
        let params = json!({ "pattern": "*.yml" });
        let result = execute_search_files(&dir, &params).unwrap();
        let files = result["files"].as_array().unwrap();
        assert_eq!(files.len(), 2);
        let first = files[0].as_str().unwrap();
        assert!(
            first.contains("newer"),
            "expected newer.yml first, got: {first}"
        );
        tokio::fs::remove_dir_all(&dir).await.ok();
    }
}
