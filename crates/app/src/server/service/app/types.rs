use garde::Validate;
use oxy::config::model::Display;
use oxy::config::validate::ValidationContext;
use oxy_shared::errors::OxyError;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use utoipa::ToSchema;

pub const APP_FILE_EXTENSION: &str = ".app.yml";
pub const APP_DATA_EXTENSION: &str = ".app.data.yml";
pub const DATA_DIR_NAME: &str = "data";
pub const TASKS_KEY: &str = "tasks";
pub const DISPLAY_KEY: &str = "display";

pub type AppResult<T> = Result<T, OxyError>;

#[derive(Serialize, Deserialize, Debug, Clone, Validate, ToSchema)]
#[garde(context(ValidationContext))]
pub struct ErrorDisplay {
    #[garde(length(min = 1))]
    pub title: String,
    #[garde(length(min = 1))]
    pub error: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, ToSchema)]
#[serde(tag = "type")]
#[garde(context(ValidationContext))]
pub enum DisplayWithError {
    #[serde(rename = "error")]
    Error(#[garde(dive)] ErrorDisplay),
    #[serde(rename = "display")]
    #[schema(value_type = Object)]
    Display(#[garde(dive)] Display),
}

/// Task execution result for the combined result endpoint
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct TaskResult {
    pub task_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Inner result containing tasks and displays
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct AppResultData {
    pub tasks: Vec<TaskResult>,
    #[schema(value_type = Vec<Object>)]
    pub displays: Vec<JsonValue>,
}

/// Response for the combined result endpoint that includes both tasks and displays
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct GetAppResultResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<AppResultData>,
}
