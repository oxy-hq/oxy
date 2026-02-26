use std::collections::HashMap;

use async_openai::types::chat::{ChatCompletionTool, FunctionObject};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SemanticQuery {
    #[serde(default = "default_semantic_query_name")]
    pub name: String,
    #[serde(default = "default_semantic_query_description")]
    pub description: String,
    #[serde(default = "default_semantic_query_instruction")]
    pub instruction: String,
    pub topic: String,
    pub model: Option<String>,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default)]
    pub variables: Option<HashMap<String, Value>>,
}

fn default_semantic_query_name() -> String {
    "semantic_query".to_string()
}

fn default_semantic_query_description() -> String {
    "Queries the database via the semantic layer and returns the results.".to_string()
}

fn default_semantic_query_instruction() -> String {
    "Generate a semantic query to retrieve the required information. Select the appropriate dimensions, measures, filters, time dimensions, and ordering based on the objective.".to_string()
}

fn default_max_retries() -> u32 {
    5
}

impl SemanticQuery {
    pub fn get_tool(&self) -> ChatCompletionTool {
        let schema = serde_json::json!(&schemars::schema_for!(oxy::types::SemanticQueryParams));
        ChatCompletionTool {
            function: FunctionObject {
                name: self.name.clone(),
                description: Some(self.description.clone()),
                parameters: Some(schema),
                strict: None,
            },
        }
    }
}
