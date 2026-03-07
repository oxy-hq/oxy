use garde::Validate;
use oxy::config::model::Display;
use oxy::config::validate::ValidationContext;
use oxy_shared::errors::OxyError;
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};
use std::collections::HashMap;
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

/// Typed output of a task execution.
/// One of: a boolean, a text string, an array of row objects (SQL result),
/// a list of nested outputs, or a named map of nested outputs.
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum TaskOutput {
    /// Boolean result (e.g. from a check task)
    #[schema(example = json!({"type": "bool", "value": true}))]
    Bool(bool),
    /// Plain text result (e.g. from an agent task)
    #[schema(example = json!({"type": "text", "value": "Query executed successfully"}))]
    Text(String),
    /// SQL query result — array of row objects
    #[schema(example = json!({"type": "table", "value": [{"id": 1, "name": "Alice", "revenue": 42000}]}))]
    Table(JsonValue),
    /// Null / missing output (preserves list positions when an item has no value)
    #[schema(example = json!({"type": "none"}))]
    None,
    /// Ordered list of nested outputs
    #[schema(value_type = Vec<Object>, example = json!({"type": "list", "value": [{"type": "text", "value": "step 1"}, {"type": "bool", "value": true}]}))]
    List(Vec<Box<TaskOutput>>),
    /// Named map of nested outputs
    #[schema(value_type = HashMap<String, Object>, example = json!({"type": "map", "value": {"query": {"type": "table", "value": [{"col": "val"}]}, "ok": {"type": "bool", "value": true}}}))]
    Map(HashMap<String, Box<TaskOutput>>),
}

/// Discriminant for the kind of task in a result.
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Agent,
    ExecuteSql,
    SemanticQuery,
    OmniQuery,
    LookerQuery,
    Loop,
    Formatter,
    SubWorkflow,
    Conditional,
    Visualize,
    Unknown,
}

impl From<&str> for TaskKind {
    fn from(s: &str) -> Self {
        match s {
            "agent" => TaskKind::Agent,
            "execute_sql" => TaskKind::ExecuteSql,
            "semantic_query" => TaskKind::SemanticQuery,
            "omni_query" => TaskKind::OmniQuery,
            "looker_query" => TaskKind::LookerQuery,
            "loop" => TaskKind::Loop,
            "formatter" => TaskKind::Formatter,
            "sub_workflow" => TaskKind::SubWorkflow,
            "conditional" => TaskKind::Conditional,
            "visualize" => TaskKind::Visualize,
            _ => TaskKind::Unknown,
        }
    }
}

/// Task execution result for the combined result endpoint
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct TaskResult {
    #[schema(example = "revenue_by_customer")]
    pub task_name: String,
    #[serde(rename = "type")]
    pub task_type: TaskKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<TaskOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = "Connection timed out")]
    pub error: Option<String>,
}

/// Inner result containing tasks and displays
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct AppResultData {
    pub tasks: Vec<TaskResult>,
    pub displays: Vec<AppResultDisplay>,
}

/// Typed display payload returned by the combined app result endpoint.
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppResultDisplay {
    LineChart(AppResultChartDisplay),
    BarChart(AppResultChartDisplay),
    PieChart(AppResultChartDisplay),
    Table(AppResultTableDisplay),
    Markdown(AppResultMarkdownDisplay),
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct AppResultChartDisplay {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct AppResultTableDisplay {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct AppResultMarkdownDisplay {
    pub content: String,
}

/// Response for the combined result endpoint that includes both tasks and displays
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
#[schema(example = json!({
    "success": true,
    "result": {
        "tasks": [
            {
                "task_name": "revenue_by_customer",
                "type   ": "execute_sql",
                "output": {
                    "type": "table",
                    "value": [{"customer": "Acme", "revenue": 42000}]
                }
            }
        ],
        "displays": [
            {
                "type": "bar_chart",
                "title": "Revenue by Customer"
            }
        ]
    }
}))]
pub struct GetAppResultResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<AppResultData>,
}
