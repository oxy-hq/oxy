use crate::{
    errors::OxyError,
    service::types::block::{Block, GroupKind},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum RunStatus {
    #[default]
    Pending,
    Running,
    Canceled,
    Completed,
    Failed,
}

impl std::fmt::Display for RunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status_str = match self {
            RunStatus::Pending => "pending",
            RunStatus::Running => "running",
            RunStatus::Canceled => "canceled",
            RunStatus::Completed => "completed",
            RunStatus::Failed => "failed",
        };
        write!(f, "{status_str}")
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema, Default)]
pub struct RunInfo {
    pub root_ref: Option<RootReference>,
    pub metadata: Option<GroupKind>,
    pub source_id: String,
    pub run_index: Option<i32>,
    pub status: RunStatus,
    #[schema(value_type = Object)]
    pub variables: Option<IndexMap<String, serde_json::Value>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl RunInfo {
    pub fn set_status(&mut self, status: RunStatus) {
        self.status = status;
    }

    pub fn is_pending(&self) -> bool {
        matches!(self.status, RunStatus::Pending)
    }

    pub fn is_completed(&self) -> bool {
        matches!(self.status, RunStatus::Completed)
    }

    pub fn task_id(&self) -> Result<String, OxyError> {
        self.run_index
            .map(|index| format!("{}::{}", self.source_id, index))
            .ok_or(OxyError::RuntimeError(
                "Run index is required to generate task ID".to_string(),
            ))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema, Default)]
pub struct RootReference {
    pub source_id: String,
    pub run_index: Option<i32>,
    pub replay_ref: String,
}

impl RootReference {
    pub fn task_id(&self) -> Result<String, OxyError> {
        self.run_index
            .map(|index| format!("{}::{}", self.source_id, index))
            .ok_or(OxyError::RuntimeError(
                "Run index is required to generate task ID".to_string(),
            ))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct RunDetails {
    #[serde(flatten)]
    pub run_info: RunInfo,
    pub blocks: Option<HashMap<String, Block>>,
    pub children: Option<Vec<String>>,
    pub error: Option<String>,
    pub output: Option<serde_json::Value>,
}
