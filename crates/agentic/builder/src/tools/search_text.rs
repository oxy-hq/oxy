use std::path::Path;

use agentic_core::tools::{ToolDef, ToolError};
use serde_json::{Value, json};

use super::utils::MAX_SEARCH_RESULTS;

pub fn search_text_def() -> ToolDef {
    ToolDef {
        name: "search_text",
        description: "Search for text (regex or literal) across project files. Returns matching lines with file path and line number. Results are capped at 50 matches.",
        parameters: json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Text to search for. Can be a regex pattern or a literal string."
                },
                "file_glob": {
                    "type": ["string", "null"],
                    "description": "Glob pattern to restrict which files are searched, e.g. '**/*.yml'. Null to search all files."
                }
            },
            "required": ["pattern", "file_glob"],
            "additionalProperties": false
        }),
    }
}

pub async fn execute_search_text(
    workspace_root: &Path,
    params: &Value,
) -> Result<Value, ToolError> {
    let pattern_str = params["pattern"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'pattern'".into()))?;

    let file_glob = params["file_glob"].as_str().unwrap_or("**/*");

    // The `regex` crate uses a finite automaton (NFA/DFA) and does not backtrack,
    // so catastrophic ReDoS from patterns like `(a+)+b` is not possible.
    // We still cap DFA/NFA memory to guard against patterns that produce enormous
    // automata (e.g. deeply nested alternations), which could otherwise exhaust RAM.
    let re = regex::RegexBuilder::new(pattern_str)
        .size_limit(1 << 20) // 1 MiB NFA size cap
        .dfa_size_limit(1 << 20) // 1 MiB DFA cache cap
        .build()
        .map_err(|e| ToolError::BadParams(format!("invalid regex pattern: {e}")))?;

    let glob_pattern = workspace_root.join(file_glob);
    let glob_str = glob_pattern
        .to_str()
        .ok_or_else(|| ToolError::BadParams("invalid file_glob encoding".into()))?;

    let paths = glob::glob(glob_str)
        .map_err(|e| ToolError::BadParams(format!("invalid file_glob: {e}")))?;

    let mut matches = Vec::new();
    'outer: for entry in paths.flatten() {
        if !entry.is_file() {
            continue;
        }
        // Skip binary files by checking extension or attempting a read.
        let Ok(content) = tokio::fs::read_to_string(&entry).await else {
            continue;
        };
        let rel = entry
            .strip_prefix(workspace_root)
            .unwrap_or(&entry)
            .to_string_lossy()
            .to_string();

        for (line_no, line) in content.lines().enumerate() {
            if re.is_match(line) {
                matches.push(json!({
                    "file": rel,
                    "line": line_no + 1,
                    "content": line.trim()
                }));
                if matches.len() >= MAX_SEARCH_RESULTS {
                    break 'outer;
                }
            }
        }
    }

    Ok(json!({
        "matches": matches,
        "count": matches.len(),
        "truncated": matches.len() >= MAX_SEARCH_RESULTS
    }))
}
