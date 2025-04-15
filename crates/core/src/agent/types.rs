use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentReference {
    SqlQuery(SqlQueryReference),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlQueryReference {
    pub sql_query: String,
    pub database: String,
    pub result: Vec<Vec<String>>,
    pub is_result_truncated: bool,
}
