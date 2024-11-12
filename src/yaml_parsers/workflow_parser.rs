use std::fs;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct AgentStep {
    pub prompt: String,
    pub agent_ref: String,
    #[serde(default = "default_retry")]
    pub retry: usize,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ExecuteSQLStep {
    pub warehouse: String,
    pub sql_file: String,
}

fn default_retry() -> usize {
    1
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum StepType {
    #[serde(rename = "agent")]
    Agent(AgentStep),
    #[serde(rename = "execute_sql")]
    ExecuteSQL(ExecuteSQLStep),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Step {
    pub name: String,
    #[serde(flatten)]
    pub step_type: StepType,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Workflow {
    pub name: String,
    pub steps: Vec<Step>,
}

// Temporary workflow object that reads in from the yaml file before it's combined with the
// workflow name (filename-associated) into the `Workflow` struct
#[derive(Deserialize)]
struct TempWorkflow {
    pub steps: Vec<Step>,
}

pub fn parse_workflow_config(workflow_name: &str, file_path: &str) -> anyhow::Result<Workflow> {
    let workflow_content = fs::read_to_string(file_path)?;
    let temp_workflow: TempWorkflow = serde_yaml::from_str(&workflow_content)?;

    let workflow = Workflow {
        name: workflow_name.to_string(),
        steps: temp_workflow.steps,
    };

    Ok(workflow)
}
