pub mod api;
pub mod cli;
pub mod types;

use types::WorkflowLogger;

use crate::execute::types::Table;

#[derive(Debug, Clone, Copy)]
pub struct NoopLogger;

impl WorkflowLogger for NoopLogger {
    fn log(&self, _text: &str) {}
    fn log_sql_query(&self, _query: &str) {}
    fn log_table_result(&self, _table: Table) {}
    fn log_text_chunk(&mut self, _chunk: &str, _is_finished: bool) {}
}
