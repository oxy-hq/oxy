use std::collections::HashMap;

use serde::Deserialize;

use super::{RouteRetrievalConfig, Task};

#[derive(Deserialize, Debug)]
pub struct AutomationWithRawVariables {
    /// Automation name. Accepted in YAML for documentation but always overwritten
    /// by the filename.
    #[serde(default)]
    pub name: String,
    pub tasks: Vec<Task>,
    pub variables: Option<HashMap<String, serde_json::Value>>,
    #[serde(default)]
    pub description: String,
    pub retrieval: Option<RouteRetrievalConfig>,
    pub consistency_prompt: Option<String>,
}

/// Back-compat alias: an automation's raw-variable form was historically named
/// `WorkflowWithRawVariables`.
pub type WorkflowWithRawVariables = AutomationWithRawVariables;
