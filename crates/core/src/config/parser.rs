use std::fs;

use crate::errors::OxyError;

use super::model::{AgentConfig, SemanticModels, TempWorkflow, Workflow};

pub fn parse_workflow_config(workflow_name: &str, file_path: &str) -> Result<Workflow, OxyError> {
    let workflow_content = fs::read_to_string(file_path)
        .map_err(|e| OxyError::ArgumentError(format!("Couldn't read workflow file: {}", e)))?;
    let temp_workflow: TempWorkflow = serde_yaml::from_str(&workflow_content).map_err(|e| {
        OxyError::ConfigurationError(format!("Couldn't parse workflow file: {}", e))
    })?;

    let workflow = Workflow {
        name: workflow_name.to_string(),
        tasks: temp_workflow.tasks,
        tests: temp_workflow.tests,
        variables: temp_workflow.variables,
        description: temp_workflow.description,
    };

    Ok(workflow)
}

pub fn parse_agent_config(file_path: &str) -> Result<AgentConfig, OxyError> {
    let agent_content = fs::read_to_string(file_path).map_err(|e| {
        OxyError::RuntimeError(format!("Unable to read agent {file_path} config: {}", e))
    })?;
    let agent: AgentConfig = serde_yaml::from_str(&agent_content).map_err(|e| {
        OxyError::ConfigurationError(format!(
            "Unable to parse agent {file_path} configuration {e}"
        ))
    })?;
    Ok(agent)
}

pub fn parse_semantic_model_config(file_path: &str) -> anyhow::Result<SemanticModels> {
    let content = fs::read_to_string(file_path)?;
    let semantic_models: SemanticModels = serde_yaml::from_str(&content)?;
    Ok(semantic_models)
}
