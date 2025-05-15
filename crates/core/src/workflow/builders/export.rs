use std::{fs::File, io::Write, path::Path, sync::Arc};

use crate::{
    config::model::{AgentTask, ExecuteSQLTask, ExportFormat, FormatterTask, TaskExport, TaskType},
    errors::OxyError,
    execute::{
        ExecutionContext,
        builders::export::Exporter,
        types::{
            Chunk, Event, EventKind, Output, OutputContainer,
            utils::{record_batches_to_json, record_batches_to_rows},
        },
        writer::{BufWriter, EventHandler},
    },
    theme::StyledText,
    utils::get_file_directories,
};
use arrow::{array::RecordBatch, datatypes::Schema};
use csv::Writer;
use tokio::{sync::mpsc::Sender, task::JoinHandle};

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

fn export_execute_sql<P: AsRef<Path>>(
    task_export: &TaskExport,
    prompt: &str,
    sql: &str,
    schema: &Arc<Schema>,
    datasets: &[RecordBatch],
    export_file_path: P,
) {
    match get_file_directories(export_file_path) {
        Ok(file_path) => {
            let result = match task_export.format {
                ExportFormat::SQL => export_sql(&file_path, prompt, sql),
                ExportFormat::CSV => export_csv(&file_path, schema, datasets),
                ExportFormat::JSON => export_json(&file_path, datasets),
                _ => {
                    tracing::warn!("Unsupported export format");
                    return;
                }
            };

            match result {
                Ok(_) => println!(
                    "{}",
                    format!("Exported to {:?}", file_path.as_ref().display()).success()
                ),
                Err(e) => println!(
                    "{}",
                    format!(
                        "Error exporting to {:?} for path '{}': {:?}",
                        task_export.format,
                        file_path.as_ref().display(),
                        e
                    )
                    .warning()
                ),
            }
        }
        Err(e) => println!(
            "{}",
            format!(
                "Error creating directories for path '{}': {}",
                task_export.path, e
            )
            .warning()
        ),
    }
}

fn export_sql<P: AsRef<Path>>(
    file_path: P,
    prompt: &str,
    sql: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::create(file_path)?;
    if !prompt.is_empty() {
        writeln!(file, "-- Prompt: {}\n", prompt)?;
    }
    file.write_all(sql.as_bytes())?;
    Ok(())
}

fn export_csv<P: AsRef<Path>>(
    file_path: P,
    schema: &Arc<Schema>,
    datasets: &[RecordBatch],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = Writer::from_path(file_path)?;
    writer.write_record(schema.fields.iter().map(|field| field.name().to_string()))?;

    let rows = record_batches_to_rows(datasets)?;
    for row in rows {
        writer.write_record(row.iter().map(|value| value.to_string()))?;
    }
    writer.flush()?;
    Ok(())
}

fn export_json<P: AsRef<Path>>(
    file_path: P,
    datasets: &[RecordBatch],
) -> Result<(), Box<dyn std::error::Error>> {
    let json_data = record_batches_to_json(datasets)?;
    std::fs::write(file_path, json_data)?;
    Ok(())
}

fn export_formatter<P: AsRef<Path>>(task_output: &str, export_file_path: P) {
    match get_file_directories(export_file_path.as_ref()) {
        Ok(file_path) => {
            let mut file = File::create(file_path).expect("Failed to create file");
            file.write_all(task_output.as_bytes())
                .expect("Failed to write to file");
            println!(
                "{}",
                format!("Exported to {:?}", file_path.display()).success()
            )
        }
        Err(e) => println!(
            "{}",
            format!(
                "Error creating directories for path '{}': {}",
                export_file_path.as_ref().display(),
                e
            )
            .warning()
        ),
    }
}
