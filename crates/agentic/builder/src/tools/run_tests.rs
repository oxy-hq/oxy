use std::path::Path;
use std::sync::Arc;

use agentic_core::tools::{ToolDef, ToolError};
use serde_json::{Value, json};

use crate::test_runner::BuilderTestRunner;

use super::utils::safe_path;

pub fn run_tests_def() -> ToolDef {
    ToolDef {
        name: "run_tests",
        description: "Run one or more Oxy test files (.test.yml) using the eval pipeline and return a summary of results (pass rate, errors). Provide a specific test file path to run a single suite, or omit to discover and run all test files in the project. Use this after proposing a test file to verify it works, or when the user asks to run tests.",
        parameters: json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": ["string", "null"],
                    "description": "Path to a .test.yml file, relative to the project root (e.g. 'agents/sales.agent.test.yml'). Null to run all discovered test files."
                }
            },
            "required": ["file_path"],
            "additionalProperties": false
        }),
        ..Default::default()
    }
}

pub async fn execute_run_tests(
    workspace_root: &Path,
    params: &Value,
    test_runner: Arc<dyn BuilderTestRunner>,
) -> Result<Value, ToolError> {
    if let Some(file_path) = params["file_path"].as_str() {
        // Run a single specified test file.
        if !file_path.ends_with(".test.yml") {
            return Err(ToolError::BadParams(format!(
                "expected a .test.yml file, got: {file_path}"
            )));
        }
        let _abs = safe_path(workspace_root, file_path)?;

        let result = test_runner
            .run_tests(workspace_root, file_path)
            .await
            .map_err(|e| ToolError::Execution(format!("test run failed: {e}")))?;

        Ok(json!({ "test_file": file_path, "result": result }))
    } else {
        // Discover and run all .test.yml files in the project.
        let glob_pattern = workspace_root.join("**/*.test.yml");
        let glob_str = glob_pattern
            .to_str()
            .ok_or_else(|| ToolError::BadParams("invalid project root encoding".into()))?;

        let mut test_files: Vec<String> = Vec::new();
        for entry in glob::glob(glob_str)
            .map_err(|e| ToolError::BadParams(format!("glob error: {e}")))?
            .flatten()
        {
            if entry.is_file() {
                let rel = entry
                    .strip_prefix(workspace_root)
                    .unwrap_or(&entry)
                    .to_string_lossy()
                    .to_string();
                test_files.push(rel);
            }
        }

        if test_files.is_empty() {
            return Ok(json!({ "message": "No .test.yml files found in the project." }));
        }

        let mut results = Vec::new();
        for file in &test_files {
            let result = test_runner
                .run_tests(workspace_root, file)
                .await
                .map_err(|e| ToolError::Execution(format!("test run failed for '{file}': {e}")))?;
            results.push(json!({ "test_file": file, "result": result }));
        }

        Ok(json!({ "tests_run": results.len(), "results": results }))
    }
}
