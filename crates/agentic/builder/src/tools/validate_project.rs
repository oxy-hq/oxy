use std::path::{Path, PathBuf};

use agentic_core::tools::{ToolDef, ToolError};
use serde_json::{json, Value};

use super::utils::safe_path;

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
    }
}

/// Validate project files using the oxy config validator.
/// Mirrors the logic of `oxy validate [--file <path>]`.
pub async fn execute_validate_project(
    project_root: &Path,
    params: &Value,
) -> Result<Value, ToolError> {
    let config = oxy::config::ConfigBuilder::new()
        .with_project_path(project_root)
        .map_err(|e| ToolError::Execution(format!("failed to configure project path: {e}")))?
        .build()
        .await
        .map_err(|e| ToolError::Execution(format!("failed to load config: {e}")))?;

    let cfg = config.get_config();

    if let Some(rel_path) = params["file_path"].as_str() {
        // Validate a single file.
        let abs = safe_path(project_root, rel_path)?;
        let file_name = abs.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let result = validate_single_file(&abs, file_name, cfg);
        match result {
            Ok(()) => Ok(json!({ "valid": true, "file": rel_path })),
            Err(e) => Ok(json!({ "valid": false, "file": rel_path, "errors": [e] })),
        }
    } else {
        // Validate all files, collecting errors.
        let mut errors: Vec<serde_json::Value> = Vec::new();
        let mut valid_count: usize = 0;

        for path in cfg.list_workflows(&cfg.project_path) {
            let rel = path
                .strip_prefix(project_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            match cfg
                .load_workflow(&path)
                .and_then(|w| cfg.validate_workflow(&w).map_err(Into::into))
            {
                Ok(()) => valid_count += 1,
                Err(e) => errors.push(json!({ "file": rel, "error": e.to_string() })),
            }
        }

        for path in cfg.list_agents(&cfg.project_path) {
            let rel = path
                .strip_prefix(project_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            match cfg
                .load_agent_config(Some(&path))
                .and_then(|(agent, name)| cfg.validate_agent(&agent, name).map_err(Into::into))
            {
                Ok(()) => valid_count += 1,
                Err(e) => errors.push(json!({ "file": rel, "error": e.to_string() })),
            }
        }

        for path in cfg.list_apps(&cfg.project_path) {
            let rel = path
                .strip_prefix(project_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            match cfg
                .load_app(&path)
                .and_then(|app| cfg.validate_app(&app).map_err(Into::into))
            {
                Ok(()) => valid_count += 1,
                Err(e) => errors.push(json!({ "file": rel, "error": e.to_string() })),
            }
        }

        for path in list_semantic_files(&cfg.project_path) {
            let rel = path
                .strip_prefix(project_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            match validate_semantic_file(&path, &cfg.project_path) {
                Ok(()) => valid_count += 1,
                Err(e) => errors.push(json!({ "file": rel, "error": e })),
            }
        }

        Ok(json!({
            "valid": errors.is_empty(),
            "valid_count": valid_count,
            "error_count": errors.len(),
            "errors": errors,
        }))
    }
}

fn validate_single_file(
    abs: &Path,
    file_name: &str,
    cfg: &oxy::config::model::Config,
) -> Result<(), String> {
    if file_name.ends_with(".procedure.yml")
        || file_name.ends_with(".workflow.yml")
        || file_name.ends_with(".automation.yml")
    {
        let w = cfg
            .load_workflow(&abs.to_path_buf())
            .map_err(|e| e.to_string())?;
        cfg.validate_workflow(&w).map_err(|e| e.to_string())
    } else if file_name.ends_with(".agent.yml") {
        let (agent, name) = cfg
            .load_agent_config(Some(&abs.to_path_buf()))
            .map_err(|e| e.to_string())?;
        cfg.validate_agent(&agent, name).map_err(|e| e.to_string())
    } else if file_name.ends_with(".app.yml") {
        let app = cfg
            .load_app(&abs.to_path_buf())
            .map_err(|e| e.to_string())?;
        cfg.validate_app(&app).map_err(|e| e.to_string())
    } else if file_name.ends_with(".view.yml") || file_name.ends_with(".topic.yml") {
        validate_semantic_file(abs, &cfg.project_path)
    } else {
        Err(format!(
            "unsupported file type: {file_name}. Expected .workflow.yml, .procedure.yml, .automation.yml, .agent.yml, .app.yml, .view.yml, or .topic.yml"
        ))
    }
}

fn validate_semantic_file(abs: &Path, project_path: &Path) -> Result<(), String> {
    let file_name = abs.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let globals_path = project_path.join("globals");
    let registry = oxy_globals::GlobalRegistry::new(globals_path);
    let semantic_base = abs
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(project_path);
    let parser_config = oxy_semantic::ParserConfig::new(semantic_base);
    let parser = oxy_semantic::SemanticLayerParser::new(parser_config, registry);
    if file_name.ends_with(".view.yml") {
        parser
            .parse_view_file(abs)
            .map(|_| ())
            .map_err(|e| e.to_string())
    } else {
        parser
            .parse_topic_file(abs)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

fn list_semantic_files(project_path: &Path) -> Vec<PathBuf> {
    let semantics_dir = project_path.join("semantics");
    let mut files = Vec::new();
    for sub in &["views", "topics"] {
        let dir = semantics_dir.join(sub);
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".view.yml") || name.ends_with(".topic.yml") {
                        files.push(path);
                    }
                }
            }
        }
    }
    files
}
