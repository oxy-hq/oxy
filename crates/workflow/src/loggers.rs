pub use crate::api_logger as api;
pub use crate::cli_logger as cli;
pub use crate::logger_types as types;

use crate::logger_types::WorkflowLogger;

use oxy::execute::types::Table;

#[derive(Debug, Clone, Copy)]
pub struct NoopLogger;

impl WorkflowLogger for NoopLogger {
    fn log(&self, _text: &str) {}
    fn log_sql_query(&self, _query: &str) {}
    fn log_table_result(&self, _table: Table) {}
    fn log_text_chunk(&mut self, _chunk: &str, _is_finished: bool) {}
    fn log_error(&self, _text: &str) {}
}
