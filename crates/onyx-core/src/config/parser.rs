use std::fs;

use crate::errors::OnyxError;

use super::model::{AgentConfig, SemanticModels, TempWorkflow, Workflow};

pub fn parse_workflow_config(workflow_name: &str, file_path: &str) -> Result<Workflow, OnyxError> {
    let workflow_content = fs::read_to_string(file_path)
        .map_err(|e| OnyxError::ArgumentError("Couldn't read workflow file".into()))?;
    let temp_workflow: TempWorkflow = serde_yaml::from_str(&workflow_content).map_err(|e| {
        OnyxError::ConfigurationError(format!("Couldn't parse workflow file: {}", e))
    })?;

    let workflow = Workflow {
        name: workflow_name.to_string(),
        steps: temp_workflow.steps,
        tests: temp_workflow.tests,
    };

    Ok(workflow)
}

pub fn parse_agent_config(file_path: &str) -> Result<AgentConfig, OnyxError> {
    let agent_content = fs::read_to_string(file_path).map_err(|e| {
        OnyxError::RuntimeError(format!("Unable to read agent {file_path} config: {}", e))
    })?;
    let agent: AgentConfig = serde_yaml::from_str(&agent_content).map_err(|e| {
        OnyxError::ConfigurationError(
            format!("Unable to parse agent {file_path} configuration {e}").into(),
        )
    })?;
    Ok(agent)
}

pub fn parse_semantic_model_config(file_path: &str) -> anyhow::Result<SemanticModels> {
    let content = fs::read_to_string(file_path)?;
    let semantic_models: SemanticModels = serde_yaml::from_str(&content)?;
    Ok(semantic_models)
}
