use indexmap::IndexMap;

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
