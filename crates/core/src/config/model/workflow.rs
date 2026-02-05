use std::collections::HashMap;

use serde::Deserialize;

use super::{EvalConfig, RouteRetrievalConfig, Task};

#[derive(Deserialize, Debug)]
pub struct WorkflowWithRawVariables {
    /// Workflow name. Accepted in YAML for documentation but always overwritten
    /// by the filename.
    #[serde(default)]
    pub name: String,
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub tests: Vec<EvalConfig>,
    pub variables: Option<HashMap<String, serde_json::Value>>,
    #[serde(default)]
    pub description: String,
    pub retrieval: Option<RouteRetrievalConfig>,
    pub consistency_prompt: Option<String>,
}
