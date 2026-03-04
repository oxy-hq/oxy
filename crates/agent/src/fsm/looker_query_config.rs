use async_openai::types::chat::{ChatCompletionTool, FunctionObject};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LookerQuery {
    #[serde(default = "default_looker_query_name")]
    pub name: String,
    #[serde(default = "default_looker_query_description")]
    pub description: String,
    #[serde(default = "default_looker_query_instruction")]
    pub instruction: String,
    pub integration: String,
    pub model: String,
    pub explore: String,
    pub query_model: Option<String>,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

fn default_looker_query_name() -> String {
    "looker_query".to_string()
}

fn default_looker_query_description() -> String {
    "Executes a Looker query on the configured integration and returns results as a table."
        .to_string()
}

fn default_looker_query_instruction() -> String {
    "Generate Looker query parameters to retrieve the required information from this explore. Use fully-qualified fields (`view.field`) and include filters/sorts/limit when helpful.".to_string()
}

fn default_max_retries() -> u32 {
    5
}

impl LookerQuery {
    pub fn get_tool(&self) -> ChatCompletionTool {
        let schema = serde_json::json!(&schemars::schema_for!(
            oxy::config::model::LookerQueryParams
        ));
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
