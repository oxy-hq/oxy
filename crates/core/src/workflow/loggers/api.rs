use std::{
    fs::File,
    io::Write,
    sync::{Arc, Mutex},
};

use crate::{
    execute::types::Table,
    service::thread::streaming_workflow_persister::StreamingWorkflowPersister,
};

use super::types::{LogItem, LogType, WorkflowLogger};

#[derive(Debug, Clone)]
pub struct WorkflowAPILogger {
    streaming_text: String,
    sender: tokio::sync::mpsc::Sender<LogItem>,
    writer: Option<Arc<Mutex<File>>>,
    streaming_persister: Option<Arc<StreamingWorkflowPersister>>,
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
            streaming_persister: None,
        }
    }

    fn log(&self, log_item: LogItem) {
        if let Some(writer) = &self.writer {
            let mut file = writer.lock().unwrap();
            let _ = writeln!(file, "{}", serde_json::to_string(&log_item).unwrap());
        }
        let _ = self.sender.try_send(log_item.clone());
        if let Some(streaming_handler) = &self.streaming_persister {
            let streaming_handler = Arc::clone(streaming_handler);
            tokio::spawn(async move {
                if let Err(e) = streaming_handler.append_output(&log_item).await {
                    eprintln!("Failed to persist log item: {}", e);
                }
            });
        }
    }

    pub fn with_streaming_persister(mut self, handler: Arc<StreamingWorkflowPersister>) -> Self {
        self.streaming_persister = Some(handler);
        self
    }
}

impl WorkflowLogger for WorkflowAPILogger {
    fn log(&self, text: &str) {
        let item = LogItem::new(strip_ansi_escapes::strip_str(text.trim()), LogType::Info);
        self.log(item)
    }

    fn log_sql_query(&self, query: &str) {
        let item = LogItem::new(format!("Query: \n\n```sql\n{}\n```", query), LogType::Info);
        self.log(item)
    }

    fn log_table_result(&self, table: Table) {
        let item = LogItem::new(format!("Result:\n\n{}", table.to_markdown()), LogType::Info);
        self.log(item);
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

    fn log_error(&self, text: &str) {
        let item = LogItem::new(strip_ansi_escapes::strip_str(text), LogType::Error);
        self.log(item)
    }
}
