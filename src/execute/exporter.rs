use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use crate::ai::utils::record_batches_to_json;
use crate::ai::utils::record_batches_to_rows;
use crate::config::model::AgentStep;
use crate::config::model::ExportFormat;
use crate::config::model::ProjectPath;
use crate::config::model::StepExport;
use crate::connector::load_result;
use crate::errors::OnyxError;
use crate::execute::agent::ToolCall;
use crate::execute::agent::ToolMetadata;
use crate::StyledText;
use arrow::array::RecordBatch;
use arrow::datatypes::Schema;
use csv::Writer;

pub fn export_agent_step(
    agent_step: &AgentStep,
    step_output: &Vec<ToolCall>,
    export_file_path: &String,
) {
    if let Some(export) = &agent_step.export {
        let mut has_execute_sql_step = false;
        for output in step_output {
            if let Some(ToolMetadata::ExecuteSQL {
                sql_query,
                output_file,
            }) = &output.metadata
            {
                let result_file_path = output_file.clone();
                let (datasets, schema) =
                    load_result(&result_file_path).expect("error to load result");
                let sql = sql_query.clone();
                let prompt = &agent_step.prompt;

                export_execute_sql(export, prompt, &sql, &schema, &datasets, export_file_path);
                has_execute_sql_step = true;
            }
        }

        if !has_execute_sql_step {
            println!("{}", "Warning: Export failed. This agent does not generate sql, so can not export anything.".warning());
        }
    }
}

pub fn export_execute_sql(
    step_export: &StepExport,
    prompt: &str,
    sql: &str,
    schema: &Arc<Schema>,
    datasets: &[RecordBatch],
    export_file_path: &String,
) {
    match get_file_directories(export_file_path) {
        Ok(file_path) => {
            let result = match step_export.format {
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
                    format!("Exported to {:?}", file_path.display()).success()
                ),
                Err(e) => println!(
                    "{}",
                    format!(
                        "Error exporting to {:?} for path '{}': {:?}",
                        step_export.format,
                        file_path.display(),
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
                step_export.path, e
            )
            .warning()
        ),
    }
}

fn get_file_directories(file_path: &String) -> Result<PathBuf, OnyxError> {
    let file_path = ProjectPath::get_path(&file_path);
    let _ = create_parent_dirs(&file_path);
    Ok(file_path)
}

fn create_parent_dirs(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn export_sql(file_path: &Path, prompt: &str, sql: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::create(file_path)?;
    if !prompt.is_empty() {
        writeln!(file, "-- Prompt: {}\n", prompt)?;
    }
    file.write_all(sql.as_bytes())?;
    Ok(())
}

fn export_csv(
    file_path: &Path,
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

fn export_json(
    file_path: &Path,
    datasets: &[RecordBatch],
) -> Result<(), Box<dyn std::error::Error>> {
    let json_data = record_batches_to_json(datasets)?;
    std::fs::write(file_path, json_data)?;
    Ok(())
}

pub fn export_formatter(step_output: &str, export_file_path: &String) {
    match get_file_directories(export_file_path) {
        Ok(file_path) => {
            let mut file = File::create(&file_path).expect("Failed to create file");
            file.write_all(step_output.as_bytes())
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
                export_file_path, e
            )
            .warning()
        ),
    }
}
