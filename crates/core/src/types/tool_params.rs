use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum OrderType {
    #[serde(rename = "asc")]
    Ascending,
    #[serde(rename = "desc")]
    Descending,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct OmniQueryParams {
    #[schemars(
        description = "Fields to select. Field name must be full name format {view}.{field_name}. No aggregation or any other syntax."
    )]
    pub fields: Vec<String>,
    #[schemars(description = "Maximum number of rows to return.")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[schemars(description = "Fields to sort by with direction (asc/desc).")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sorts: Option<HashMap<String, OrderType>>,
}

impl Hash for OmniQueryParams {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.fields.hash(state);
        self.limit.hash(state);
        // Sort the hashmap keys to ensure consistent hashing
        if let Some(sorts) = &self.sorts {
            let mut sorted: Vec<_> = sorts.iter().collect();
            sorted.sort_by_key(|(k, _)| *k);
            for (k, v) in sorted {
                k.hash(state);
                v.hash(state);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RetrievalParams {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SQLParams {
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentParams {
    pub agent_name: String,
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmptySQLParams {}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SaveAutomationParams {
    #[schemars(description = "A descriptive name for the automation being saved")]
    pub name: String,
    #[schemars(description = "A description of what this automation does")]
    pub description: String,
}
