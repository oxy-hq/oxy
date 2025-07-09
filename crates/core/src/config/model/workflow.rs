use std::collections::HashMap;

use serde::Deserialize;

use super::{EvalConfig, Task, RouteRetrievalConfig};

#[derive(Deserialize, Debug)]
pub struct WorkflowWithRawVariables {
    #[serde(skip)]
    pub name: String,
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub tests: Vec<EvalConfig>,
    pub variables: Option<HashMap<String, serde_json::Value>>,
    #[serde(default)]
    pub description: String,
    pub retrieval: Option<RouteRetrievalConfig>,
}
