use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use crate::ai::utils::record_batches_to_json;
use crate::ai::utils::record_batches_to_rows;
use crate::config::model::AgentTask;
use crate::config::model::ExportFormat;
use crate::config::model::TaskExport;
use crate::connector::load_result;
use crate::errors::OnyxError;
use crate::execute::agent::ToolCall;
use crate::execute::agent::ToolMetadata;
use crate::StyledText;
use arrow::array::RecordBatch;
use arrow::datatypes::Schema;
use csv::Writer;

pub fn export_agent_task<P: AsRef<Path>>(
    agent_task: &AgentTask,
    task_output: &[&ToolCall],
    export_file_path: P,
) {
    if let Some(export) = &agent_task.export {
        let mut has_execute_sql_task = false;
        for output in task_output {
            if let Some(ToolMetadata::ExecuteSQL {
                sql_query,
                output_file,
            }) = &output.metadata
            {
                let result_file_path = output_file.clone();
                let (datasets, schema) =
                    load_result(&result_file_path).expect("error to load result");
                let sql = sql_query.clone();
                let prompt = &agent_task.prompt;

                export_execute_sql(
                    export,
                    prompt,
                    &sql,
                    &schema,
                    &datasets,
                    export_file_path.as_ref(),
                );
                has_execute_sql_task = true;
            }
        }

        if !has_execute_sql_task {
            println!("{}", "Warning: Export failed. This agent does not generate sql, so can not export anything.".warning());
        }
    }
}

pub fn export_execute_sql<P: AsRef<Path>>(
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
                    log::warn!("Unsupported export format");
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

pub fn get_file_directories<P: AsRef<Path>>(file_path: P) -> Result<P, OnyxError> {
    create_parent_dirs(&file_path).map_err(|e| {
        OnyxError::IOError(format!(
            "Error creating directories for path '{}': {}",
            file_path.as_ref().display(),
            e
        ))
    })?;
    Ok(file_path)
}

fn create_parent_dirs<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
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

pub fn export_formatter<P: AsRef<Path>>(task_output: &str, export_file_path: P) {
    match get_file_directories(export_file_path.as_ref()) {
        Ok(file_path) => {
            let mut file = File::create(&file_path).expect("Failed to create file");
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
