use async_openai::types::chat::{ChatCompletionTool, FunctionObject};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Query {
    #[serde(default = "default_query_name")]
    pub name: String,
    #[serde(default = "default_query_description")]
    pub description: String,
    #[serde(default = "default_query_instruction")]
    pub instruction: String,
    pub database: String,
    pub model: Option<String>,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

fn default_query_name() -> String {
    "query".to_string()
}

fn default_query_description() -> String {
    "Executes a SQL query on the connected database and returns the results.".to_string()
}

fn default_query_instruction() -> String {
    "Generate a SQL query to retrieve the required information from the database. Ensure the query is syntactically correct and optimized for performance.".to_string()
}

fn default_max_retries() -> u32 {
    5
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SQLParams {
    #[schemars(
        description = "The name of the SQL query to execute. This should be short and descriptive."
    )]
    pub title: String,
    #[schemars(description = "The SQL query to execute")]
    pub sql: String,
}

impl Query {
    pub fn get_tool(&self) -> ChatCompletionTool {
        let schema = serde_json::json!(&schemars::schema_for!(SQLParams));
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
