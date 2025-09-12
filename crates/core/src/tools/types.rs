use std::collections::HashMap;

use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionTool, ChatCompletionToolArgs, FunctionObject,
    FunctionObjectArgs,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::model::omni::OmniSemanticModel;
use crate::config::model::{EmbeddingConfig, VectorDBConfig, WorkflowTool};

use super::create_data_app::types::CreateDataAppParams;
use super::visualize::types::VisualizeParams;

#[derive(Debug, Clone)]
pub struct ToolRawInput {
    pub call_id: String,
    pub handle: String,
    pub param: String,
}

#[derive(Debug)]
pub struct OmniTopicInfoInput {
    pub topic: String,
    pub semantic_model: OmniSemanticModel,
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

#[derive(Debug, Deserialize, JsonSchema, Clone)]
pub enum OrderType {
    #[serde(rename = "asc")]
    Ascending,
    #[serde(rename = "desc")]
    Descending,
}
#[derive(Debug)]
pub struct OmniInput {
    pub database: String,
    pub params: ExecuteOmniParams,
    pub semantic_model: OmniSemanticModel,
}

#[derive(Debug, Deserialize, JsonSchema, Clone)]
pub enum FilterOperator {
    #[serde(rename = "eq")]
    Equal,
    #[serde(rename = "neq")]
    NotEqual,
    #[serde(rename = "gt")]
    GreaterThan,
    #[serde(rename = "gte")]
    GreaterThanOrEqual,
    #[serde(rename = "lt")]
    LessThan,
    #[serde(rename = "lte")]
    LessThanOrEqual,
    #[serde(rename = "in")]
    In,
    #[serde(rename = "not_in")]
    NotIn,
}

#[derive(Debug, Deserialize, JsonSchema, Clone)]
pub struct Filter {
    #[schemars(
        description = "Field name must be full name format {view}.{field_name}. No aggregation or any other syntax."
    )]
    pub field: String,
    #[schemars(description = "The operator to use for filtering.")]
    pub operator: FilterOperator,
    #[schemars(
        description = "Values to filter, example: true, 12, 'abc', (1,2,3). String must be quoted with single quotes"
    )]
    pub values: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExecuteOmniParams {
    #[schemars(description = "The topic to query from.")]
    pub topic: String,
    #[schemars(
        description = "You can only select field name. Field name must be full name format {view}.{field_name}. No aggregation or any other syntax."
    )]
    pub fields: Vec<String>,
    #[schemars(description = "List of the filters.")]
    pub filters: Vec<Filter>,
    pub limit: Option<u64>,
    #[schemars(description = "List of the sorts.")]
    pub sorts: Option<HashMap<String, OrderType>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OmniTopicInfoParams {
    pub topic: String,
}

#[derive(Debug, Deserialize, JsonSchema, Clone)]
pub struct OmniQueryParams {
    #[schemars(
        description = "The topic to query from. If not specified, uses the tool's configured topic."
    )]
    pub topic: Option<String>,
    #[schemars(
        description = "Fields to select. Field name must be full name format {view}.{field_name}. No aggregation or any other syntax."
    )]
    pub fields: Vec<String>,
    #[schemars(description = "List of filters to apply to the query.")]
    pub filters: Option<Vec<Filter>>,
    #[schemars(description = "Maximum number of rows to return.")]
    pub limit: Option<u64>,
    #[schemars(description = "Fields to sort by with direction (asc/desc).")]
    pub sorts: Option<HashMap<String, OrderType>>,
}

#[derive(Debug)]
pub struct SQLInput {
    pub database: String,
    pub sql: String,
    pub dry_run_limit: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AgentParams {
    #[schemars(description = "Chat with your prompt")]
    pub prompt: String,
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
        ChatCompletionToolArgs::default()
            .function::<FunctionObject>(val.into())
            .build()
            .unwrap()
    }
}
