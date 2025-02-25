use super::model::{Config, ExportFormat, TaskExport, TaskType};
use std::{env, fmt::Display, path::PathBuf};

const FILE_NOT_FOUND_ERROR: &str = "file does not exist";
const DIR_NOT_FOUND_ERROR: &str = "directory does not exist";
const ENV_VAR_NOT_FOUND_ERROR: &str = "env var not set";
const SQL_FILE_NOT_FOUND_ERROR: &str = "sql file not found";
const DATABASE_NOT_FOUND_ERROR: &str = "database not found";
const AGENT_NOT_FOUND_ERROR: &str = "agent not found";

fn format_error_message(error_message: &str, value: impl Display) -> garde::Error {
    garde::Error::new(format!("{} ({})", error_message, value))
}

pub fn validate_file_path(path: &PathBuf, context: &ValidationContext) -> garde::Result {
    if path.is_absolute() || path.components().count() > 1 {
        return Err(format_error_message(
            "File must be in the current directory",
            path.as_path().to_string_lossy(),
        ));
    }

    let file_path = context.config.project_path.join(path);

    if !file_path.exists() {
        return Err(format_error_message(
            FILE_NOT_FOUND_ERROR,
            file_path.as_path().to_string_lossy(),
        ));
    }
    Ok(())
}

pub fn validation_directory_path(path: &PathBuf, _: &ValidationContext) -> garde::Result {
    if !path.is_dir() {
        return Err(format_error_message(
            DIR_NOT_FOUND_ERROR,
            path.as_path().to_string_lossy(),
        ));
    }
    Ok(())
}

pub fn validate_env_var(env_var: &str, _: &ValidationContext) -> garde::Result {
    match env::var(env_var) {
        Ok(_) => Ok(()),
        Err(_) => Err(format_error_message(ENV_VAR_NOT_FOUND_ERROR, env_var)),
    }
}

pub struct ValidationContext {
    pub config: Config,
}

pub fn validate_database_exists(database_name: &str, context: &ValidationContext) -> garde::Result {
    let database = context.config.find_database(database_name);
    match database {
        Ok(_) => Ok(()),
        Err(_) => Err(format_error_message(
            DATABASE_NOT_FOUND_ERROR,
            database_name,
        )),
    }
}

pub fn validate_sql_file(sql_file: &str, context: &ValidationContext) -> garde::Result {
    let path = &context.config.project_path.join(sql_file);
    if !path.exists() {
        return Err(format_error_message(
            SQL_FILE_NOT_FOUND_ERROR,
            path.as_path().to_string_lossy(),
        ));
    }
    Ok(())
}

pub fn validate_agent_exists(agent: &str, context: &ValidationContext) -> garde::Result {
    let path = &context.config.project_path.join(agent);
    if !path.exists() {
        return Err(format_error_message(
            AGENT_NOT_FOUND_ERROR,
            path.as_path().to_string_lossy(),
        ));
    }
    Ok(())
}

pub fn validate_task(task_type: &TaskType, _context: &ValidationContext) -> garde::Result {
    match task_type {
        TaskType::Agent(task) => validate_export(
            task.export.as_ref(),
            &[ExportFormat::JSON, ExportFormat::CSV, ExportFormat::SQL],
            "agent",
        ),
        TaskType::ExecuteSQL(task) => validate_export(
            task.export.as_ref(),
            &[ExportFormat::JSON, ExportFormat::CSV, ExportFormat::SQL],
            "ExecuteSQL",
        ),
        TaskType::Formatter(task) => validate_export(
            task.export.as_ref(),
            &[ExportFormat::TXT, ExportFormat::DOCX],
            "Formatter",
        ),
        TaskType::LoopSequential(_) | TaskType::Unknown => Ok(()),
    }
}

fn validate_export(
    export: Option<&TaskExport>,
    allowed_formats: &[ExportFormat],
    task_name: &str,
) -> garde::Result {
    if let Some(export) = export {
        if !allowed_formats.contains(&export.format) {
            return Err(garde::Error::new(format!(
                "Invalid export format: {:?}, only supports {:?} for {} task",
                export.format, allowed_formats, task_name
            )));
        }
    }
    Ok(())
}
