use std::{
    fs::File,
    io::Write,
    sync::{Arc, Mutex},
};

use crate::execute::types::Table;

use super::types::{LogItem, LogType, WorkflowLogger};

#[derive(Debug, Clone)]
pub struct WorkflowAPILogger {
    streaming_text: String,
    sender: tokio::sync::mpsc::Sender<LogItem>,
    writer: Option<Arc<Mutex<File>>>,
}

impl WorkflowAPILogger {
    pub fn new(
        sender: tokio::sync::mpsc::Sender<LogItem>,
        writer: Option<Arc<Mutex<File>>>,
    ) -> Self {
        Self {
            sender,
            writer,
            streaming_text: String::new(),
        }
    }

    pub fn log(&self, log_item: LogItem) {
        if let Some(writer) = &self.writer {
            let mut file = writer.lock().unwrap();
            let _ = writeln!(file, "{}", serde_json::to_string(&log_item).unwrap());
        }
        let _ = self.sender.try_send(log_item);
    }
}

impl WorkflowLogger for WorkflowAPILogger {
    fn log(&self, text: &str) {
        let item = LogItem::new(strip_ansi_escapes::strip_str(text), LogType::Info);
        self.log(item)
    }

    fn log_sql_query(&self, query: &str) {
        let item = LogItem::new(format!("Query: \n\n```sql\n{}\n```", query), LogType::Info);
        self.log(item)
    }

    fn log_table_result(&self, table: Table) {
        match table.to_markdown() {
            Ok(table) => {
                let item = LogItem::new(format!("Result:\n\n{}", table), LogType::Info);
                self.log(item);
            }
            Err(e) => {
                let err_log =
                    LogItem::new(format!("Error displaying results: {}", e), LogType::Error);
                self.log(err_log);
            }
        }
    }

    fn log_text_chunk(&mut self, chunk: &str, is_finished: bool) {
        self.streaming_text.push_str(chunk);
        if !is_finished {
            return;
        }
        let text = std::mem::take(&mut self.streaming_text);
        let item = LogItem::new(text, LogType::Info);
        self.log(item)
    }
}
