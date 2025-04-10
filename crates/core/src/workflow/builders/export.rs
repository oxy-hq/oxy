use tokio::{sync::mpsc::Sender, task::JoinHandle};

use crate::{
    config::model::{AgentTask, ExecuteSQLTask, FormatterTask, TaskExport, TaskType},
    errors::OxyError,
    execute::{
        ExecutionContext,
        builders::export::Exporter,
        exporter::{export_execute_sql, export_formatter},
        types::{Chunk, Event, EventKind, Output, OutputContainer},
        writer::{BufWriter, EventHandler},
    },
};

use super::task::TaskInput;

#[derive(Clone)]
pub(super) struct TaskExporter;

pub struct ExporterEventHandler {
    tx: Sender<Event>,
    prompt: String,
    export_info: TaskExport,
}

#[async_trait::async_trait]
impl EventHandler for ExporterEventHandler {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        if let EventKind::Updated {
            chunk:
                Chunk {
                    key: None,
                    delta: Output::Table(table),
                    finished: _,
                },
        } = &event.kind
        {
            if let Some((sql, schema, batches)) = table.to_export() {
                export_execute_sql(
                    &self.export_info,
                    &self.prompt,
                    &sql,
                    schema,
                    batches,
                    &self.export_info.path,
                );
            }
        }
        self.tx.send(event).await.map_err(|_| {
            OxyError::RuntimeError("Failed to send event from exporter".to_string())
        })?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl Exporter<TaskInput, OutputContainer> for TaskExporter {
    async fn should_export(
        &self,
        _execution_context: &ExecutionContext,
        input: &TaskInput,
    ) -> bool {
        let export_info = match &input.task.task_type {
            TaskType::Agent(AgentTask { export, .. }) => export,
            TaskType::ExecuteSQL(ExecuteSQLTask { export, .. }) => export,
            TaskType::Formatter(FormatterTask { export, .. }) => export,
            _ => &None,
        };
        return export_info.is_some();
    }
    async fn export(
        &self,
        execution_context: &ExecutionContext,
        buf_writer: BufWriter,
        input: TaskInput,
        output_handle: JoinHandle<Result<OutputContainer, OxyError>>,
    ) -> Result<OutputContainer, OxyError> {
        let (export_info, prompt) = match input.task.task_type {
            TaskType::Agent(AgentTask { prompt, export, .. }) => (export, prompt),
            TaskType::ExecuteSQL(ExecuteSQLTask { export, .. }) => (export, String::new()),
            TaskType::Formatter(FormatterTask { export, .. }) => (export, String::new()),
            _ => (None, String::new()),
        };
        let mut export_info = export_info.unwrap();
        let path_resolver: Result<String, OxyError> = {
            let rendered_path = execution_context.renderer.render(&export_info.path)?;
            let final_path = execution_context.config.resolve_file(rendered_path).await?;
            Ok(final_path)
        };
        export_info.path = match path_resolver {
            Ok(path) => path,
            Err(_) => export_info.path,
        };
        let event_handler = ExporterEventHandler {
            tx: execution_context.writer.clone(),
            export_info: export_info.clone(),
            prompt,
        };
        let event_handle =
            tokio::spawn(async move { buf_writer.write_to_handler(event_handler).await });
        let output = output_handle.await?;
        event_handle.await??;
        let output = output?;
        if let OutputContainer::Single(Output::Text(text)) = &output {
            export_formatter(text, export_info.path);
        }
        Ok(output)
    }
}
