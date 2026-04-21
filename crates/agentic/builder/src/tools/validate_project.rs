use std::path::Path;

use agentic_core::tools::{ToolDef, ToolError};
use serde_json::{Value, json};

use super::utils::safe_path;
use crate::validator::BuilderProjectValidator;

pub fn validate_project_def() -> ToolDef {
    ToolDef {
        name: "validate_project",
        description: "Validate project configuration files (agents, workflows, apps, semantic views, topics) against the Oxy schema. Optionally validate a single file by providing its path. Returns a list of validation errors or confirms all files are valid.",
        parameters: json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": ["string", "null"],
                    "description": "Path to a specific file to validate, relative to the project root. Null to validate all agents, workflows, apps, and semantic layer files."
                }
            },
            "required": ["file_path"],
            "additionalProperties": false
        }),
        ..Default::default()
    }
}

/// Validate project files using the project validator trait.
pub async fn execute_validate_project(
    project_root: &Path,
    params: &Value,
    validator: &dyn BuilderProjectValidator,
) -> Result<Value, ToolError> {
    if let Some(rel_path) = params["file_path"].as_str() {
        // Validate a single file.
        let abs = safe_path(project_root, rel_path)?;
        match validator.validate_file(&abs).await {
            Ok(()) => Ok(json!({ "valid": true, "file": rel_path })),
            Err(e) => Ok(json!({ "valid": false, "file": rel_path, "errors": [e] })),
        }
    } else {
        // Validate all files.
        let report = validator.validate_all().await?;
        let error_values: Vec<Value> = report
            .errors
            .iter()
            .filter_map(|f| {
                f.error
                    .as_ref()
                    .map(|e| json!({ "file": f.relative_path, "error": e }))
            })
            .collect();

        Ok(json!({
            "valid": error_values.is_empty(),
            "valid_count": report.valid_count,
            "error_count": error_values.len(),
            "errors": error_values,
        }))
    }
}
