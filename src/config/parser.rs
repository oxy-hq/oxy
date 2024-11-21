use std::fs;

use super::model::{AgentConfig, TempWorkflow, Workflow};

pub fn parse_workflow_config(workflow_name: &str, file_path: &str) -> anyhow::Result<Workflow> {
    let workflow_content = fs::read_to_string(file_path)?;
    let temp_workflow: TempWorkflow = serde_yaml::from_str(&workflow_content)?;

    let workflow = Workflow {
        name: workflow_name.to_string(),
        steps: temp_workflow.steps,
    };

    Ok(workflow)
}

pub fn parse_agent_config(file_path: &str) -> anyhow::Result<AgentConfig> {
    let agent_content = fs::read_to_string(file_path)?;
    let agent: AgentConfig = serde_yaml::from_str(&agent_content)?;
    Ok(agent)
}
