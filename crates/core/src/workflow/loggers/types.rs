use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::execute::types::Table;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum LogType {
    #[serde(rename = "success")]
    Success,
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "warning")]
    Warning,
    #[serde(rename = "error")]
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LogItem {
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub log_type: LogType,
}

impl LogItem {
    pub fn new(content: String, log_type: LogType) -> Self {
        Self {
            content,
            timestamp: Utc::now(),
            log_type,
        }
    }
}
pub trait WorkflowLogger: Send + Sync {
    fn log(&self, text: &str);
    fn log_sql_query(&self, query: &str);
    fn log_table_result(&self, table: Table);
    fn log_text_chunk(&mut self, chunk: &str, is_finished: bool);
}
