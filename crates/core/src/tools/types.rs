use std::collections::HashMap;

use async_openai::types::chat::{
    ChatCompletionMessageToolCall, ChatCompletionTool, FunctionObject, FunctionObjectArgs,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// Types moved to create_data_app and visualize modules
// pub struct VisualizeInput and pub struct CreateDataAppInput
// have been removed as those modules don't exist in the refactored structure

#[derive(Debug, Clone, Serialize)]
pub struct ToolRawInput {
    pub call_id: String,
    pub handle: String,
    pub param: String,
}

impl From<&ChatCompletionMessageToolCall> for ToolRawInput {
    fn from(call: &ChatCompletionMessageToolCall) -> Self {
        ToolRawInput {
            call_id: call.id.to_string(),
            handle: call.function.name.to_string(),
            param: call.function.arguments.to_string(),
        }
    }
}

impl From<ChatCompletionMessageToolCall> for ToolRawInput {
    fn from(call: ChatCompletionMessageToolCall) -> Self {
        (&call).into()
    }
}

/// Retrieval input for vector search
#[derive(Debug, Clone, Serialize)]
pub struct RetrievalInput {
    pub query: String,
    pub agent_name: String,
    pub retrieval_config: crate::config::model::RetrievalConfig,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RetrievalParams {
    pub query: String,
}

// OmniQueryParams and OrderType moved to crate::types::tool_params

#[derive(Debug, Serialize)]
pub struct SQLInput {
    pub name: Option<String>,
    pub database: String,
    pub sql: String,
    pub dry_run_limit: Option<u64>,
    pub persist: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AgentParams {
    #[schemars(description = "Chat with your prompt")]
    pub prompt: String,
    pub variables: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateV0AppParams {
    #[schemars(description = "Use to set the app name when create the v0 app.")]
    pub name: Option<String>,
    #[schemars(description = "
Prompt to create or update the v0 app.
Include tables when needed by include their file_path values (e.g., 'Use sales table at file_path: tables/0.parquet') so v0 can query them via Oxy SDK.
DO NOT use the csv path directly (e.g., 'Use sales table at /tmp/xyz/sales.csv') as it won't work.")]
    pub prompt: String,
}

// VisualizeInput and CreateDataAppInput removed - modules don't exist

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SQLParams {
    #[serde(alias = "query")]
    pub sql: String,
    #[serde(default)]
    #[schemars(
        description = "Enable when needed to build data apps or charts based on the query result."
    )]
    pub persist: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EmptySQLParams {}

pub struct ToolSpec {
    pub handle: String,
    pub description: String,
    pub param_schema: serde_json::Value,
}

impl From<&ToolSpec> for FunctionObject {
    fn from(val: &ToolSpec) -> Self {
        FunctionObjectArgs::default()
            .name(val.handle.clone())
            .description(val.description.clone())
            .parameters(val.param_schema.clone())
            .build()
            .unwrap()
    }
}

impl From<&ToolSpec> for ChatCompletionTool {
    fn from(val: &ToolSpec) -> Self {
        ChatCompletionTool {
            function: FunctionObject::from(val),
        }
    }
}
