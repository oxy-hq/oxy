use std::collections::HashMap;
use std::hash::Hash;

use async_openai::types::chat::{
    ChatCompletionMessageToolCall, ChatCompletionTool, FunctionObject, FunctionObjectArgs,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::model::{EmbeddingConfig, VectorDBConfig, WorkflowTool};

use super::create_data_app::types::CreateDataAppParams;
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

pub struct RetrievalInput<C> {
    pub query: String,
    pub db_config: VectorDBConfig,
    pub db_name: String,
    pub openai_config: C,
    pub embedding_config: EmbeddingConfig,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RetrievalParams {
    pub query: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone, Hash)]
pub enum OrderType {
    #[serde(rename = "asc")]
    Ascending,
    #[serde(rename = "desc")]
    Descending,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
pub struct OmniQueryParams {
    #[schemars(
        description = "Fields to select. Field name must be full name format {view}.{field_name}. No aggregation or any other syntax."
    )]
    pub fields: Vec<String>,
    #[schemars(description = "Maximum number of rows to return.")]
    pub limit: Option<u64>,
    #[schemars(description = "Fields to sort by with direction (asc/desc).")]
    pub sorts: Option<HashMap<String, OrderType>>,
}

impl Hash for OmniQueryParams {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.fields.hash(state);
        self.limit.hash(state);
        if let Some(sorts) = &self.sorts {
            for (k, v) in sorts {
                k.hash(state);
                v.hash(state);
            }
        }
    }
}

#[derive(Debug)]
pub struct SQLInput {
    pub name: Option<String>,
    pub database: String,
    pub sql: String,
    pub dry_run_limit: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AgentParams {
    #[schemars(description = "Chat with your prompt")]
    pub prompt: String,
    pub variables: Option<HashMap<String, serde_json::Value>>,
}

pub struct VisualizeInput {
    pub param: VisualizeParams,
}

pub struct WorkflowInput {
    pub workflow_config: WorkflowTool,
    pub variables: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateDataAppInput {
    pub param: CreateDataAppParams,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SQLParams {
    pub sql: String,
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
