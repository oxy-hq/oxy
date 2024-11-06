use std::fs;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Step {
    pub name: String,
    pub prompt: String,
    pub agent_ref: String,
    #[serde(default = "default_retry")]
    pub retry: usize,
}

fn default_retry() -> usize {
    1
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Workflow {
    pub name: String,
    pub steps: Vec<Step>,
}

pub fn parse_workflow_config(file_path: &str) -> anyhow::Result<Workflow> {
    let workflow_content = fs::read_to_string(file_path)?;
    let workflow: Workflow = serde_yaml::from_str(&workflow_content)?;
    Ok(workflow)
}
