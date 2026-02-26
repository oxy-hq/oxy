use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use oxy::config::model::RouteRetrievalConfig;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SaveAutomation {
    #[serde(default = "default_save_automation_name")]
    pub name: String,
    #[serde(default = "default_save_automation_description")]
    pub description: String,
    #[serde(default)]
    pub retrieval: Option<RouteRetrievalConfig>,
}

fn default_save_automation_name() -> String {
    "save_automation".to_string()
}

fn default_save_automation_description() -> String {
    "Saves the current reasoning path as a reusable automation that can be executed deterministically."
        .to_string()
}
