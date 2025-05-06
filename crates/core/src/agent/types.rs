use crate::config::model::AgentTask;

#[derive(Debug, Clone)]
pub struct AgentInput {
    pub agent_ref: String,
    pub prompt: String,
}

impl From<&AgentTask> for AgentInput {
    fn from(task: &AgentTask) -> Self {
        Self {
            agent_ref: task.agent_ref.clone(),
            prompt: task.prompt.clone(),
        }
    }
}
