use std::collections::HashMap;

use serde_json::Value;

use crate::{config::model::AgentTask, service::agent::Message};

#[derive(Debug, Clone)]
pub struct AgentInput {
    pub agent_ref: String,
    pub prompt: String,
    pub memory: Vec<Message>,
    /// Runtime variables to pass to the agent
    pub variables: Option<HashMap<String, Value>>,
}

impl From<&AgentTask> for AgentInput {
    fn from(task: &AgentTask) -> Self {
        Self {
            agent_ref: task.agent_ref.clone(),
            prompt: task.prompt.clone(),
            memory: vec![],
            variables: task.variables.clone(),
        }
    }
}
