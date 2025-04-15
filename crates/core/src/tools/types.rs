use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionTool, ChatCompletionToolArgs, FunctionObject,
    FunctionObjectArgs,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::config::model::RetrievalConfig;

use super::visualize::types::VisualizeParams;

#[derive(Debug, Clone)]
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

pub struct RetrievalInput {
    pub agent_name: String,
    pub retrieval_config: RetrievalConfig,
    pub query: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RetrievalParams {
    pub query: String,
}

pub struct SQLInput {
    pub database: String,
    pub sql: String,
}

pub struct VisualizeInput {
    pub param: VisualizeParams,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SQLParams {
    pub sql: String,
}

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
        ChatCompletionToolArgs::default()
            .function::<FunctionObject>(val.into())
            .build()
            .unwrap()
    }
}
