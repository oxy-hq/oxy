use std::collections::HashMap;

use serde_json::Value;

use crate::{
    config::model::AgentTask, execute::types::event::SandboxInfo, service::agent::Message,
};

#[derive(Debug, Clone)]
pub struct AgentInput {
    pub agent_ref: String,
    pub prompt: String,
    pub memory: Vec<Message>,
    /// Runtime variables to pass to the agent
    pub variables: Option<HashMap<String, Value>>,
    /// A2A task ID for tracking (optional, only used in A2A context)
    pub a2a_task_id: Option<String>,
    /// A2A thread ID for conversation continuity (optional, only used in A2A context)
    pub a2a_thread_id: Option<String>,
    /// A2A context ID for grouping related tasks (optional, only used in A2A context)
    pub a2a_context_id: Option<String>,
    /// Sandbox information from thread (e.g., v0 chat_id and preview_url)
    pub sandbox_info: Option<SandboxInfo>,
}

impl From<&AgentTask> for AgentInput {
    fn from(task: &AgentTask) -> Self {
        Self {
            agent_ref: task.agent_ref.clone(),
            prompt: task.prompt.clone(),
            memory: vec![],
            variables: task.variables.clone(),
            a2a_task_id: None,
            a2a_thread_id: None,
            a2a_context_id: None,
            sandbox_info: None,
        }
    }
}
