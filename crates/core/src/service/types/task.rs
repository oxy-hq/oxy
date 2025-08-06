use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum TaskMetadata {
    SubWorkflow { workflow_id: String, run_id: u32 },
    Loop { values: Vec<serde_json::Value> },
    LoopItem { index: usize },
}
