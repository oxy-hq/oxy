use std::fs;

use oxy_shared::errors::OxyError;

use super::model::{Automation, SemanticModels};

pub fn parse_automation_config(
    automation_name: &str,
    file_path: &str,
) -> Result<Automation, OxyError> {
    let automation_content = fs::read_to_string(file_path).map_err(|e| {
        OxyError::ArgumentError(format!("Couldn't read automation file {file_path}: {e}"))
    })?;
    let mut automation: Automation = serde_yaml::from_str(&automation_content).map_err(|e| {
        OxyError::ConfigurationError(format!("Couldn't parse automation file {file_path}: {e}"))
    })?;

    // Name is always derived from the filename, not the YAML content
    automation.name = automation_name.to_string();

    Ok(automation)
}

pub fn parse_semantic_model_config(file_path: &str) -> anyhow::Result<SemanticModels> {
    let content = fs::read_to_string(file_path)?;
    let semantic_models: SemanticModels = serde_yaml::from_str(&content)?;
    Ok(semantic_models)
}
