use indexmap::IndexMap;

use crate::types::run::RootReference;

#[derive(Debug, Clone, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum RetryStrategy {
    RetryWithVariables {
        replay_id: Option<String>,
        run_index: u32,
        #[schema(value_type = Object)]
        variables: Option<IndexMap<String, serde_json::Value>>,
    },
    Retry {
        replay_id: Option<String>,
        run_index: u32,
    },
    LastFailure,
    NoRetry {
        #[schema(value_type = Object)]
        variables: Option<IndexMap<String, serde_json::Value>>,
    },
    Preview,
}

impl RetryStrategy {
    pub fn replay_id(&self, root_ref: &Option<RootReference>) -> Option<String> {
        match self {
            RetryStrategy::RetryWithVariables { replay_id, .. } => replay_id.clone(),
            RetryStrategy::Retry { replay_id, .. } => replay_id.clone(),
            _ => None,
        }
        .map(|id| {
            if let Some(root) = root_ref {
                if id.is_empty() {
                    return root.replay_ref.clone();
                }
                format!("{}.{}", root.replay_ref, id)
            } else {
                id
            }
        })
    }

    pub fn run_index(&self) -> Option<u32> {
        match self {
            RetryStrategy::RetryWithVariables { run_index, .. } => Some(*run_index),
            RetryStrategy::Retry { run_index, .. } => Some(*run_index),
            _ => None,
        }
    }
}
