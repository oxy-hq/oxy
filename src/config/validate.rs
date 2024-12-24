use super::model::{Config, ProjectPath};
use std::{env, fmt::Display, path::PathBuf};

const FILE_NOT_FOUND_ERROR: &str = "file does not exist";
const DIR_NOT_FOUND_ERROR: &str = "directory does not exist";
const ENV_VAR_NOT_FOUND_ERROR: &str = "env var not set";
const SQL_FILE_NOT_FOUND_ERROR: &str = "sql file not found";
const WAREHOUSE_NOT_FOUND_ERROR: &str = "warehouse not found";
const AGENT_NOT_FOUND_ERROR: &str = "agent not found";

fn format_error_message(error_message: &str, value: impl Display) -> garde::Error {
    garde::Error::new(format!("{} ({})", error_message, value))
}

pub fn validate_file_path(path: &PathBuf, _: &ValidationContext) -> garde::Result {
    if path.is_absolute() || path.components().count() > 1 {
        return Err(format_error_message(
            "File must be in the current directory",
            path.as_path().to_string_lossy(),
        ));
    }

    let file_path = ProjectPath::get_path(&path.as_path().to_string_lossy());

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

pub fn validate_warehouse_exists(
    warehouse_name: &str,
    context: &ValidationContext,
) -> garde::Result {
    let warehouse = context.config.find_warehouse(warehouse_name);
    match warehouse {
        Ok(_) => Ok(()),
        Err(_) => Err(format_error_message(
            WAREHOUSE_NOT_FOUND_ERROR,
            warehouse_name,
        )),
    }
}

pub fn validate_sql_file(sql_file: &str, _context: &ValidationContext) -> garde::Result {
    let path = &ProjectPath::get_path(sql_file);
    if !path.exists() {
        return Err(format_error_message(
            SQL_FILE_NOT_FOUND_ERROR,
            path.as_path().to_string_lossy(),
        ));
    }
    Ok(())
}

pub fn validate_agent_exists(agent: &str, _context: &ValidationContext) -> garde::Result {
    let path = &ProjectPath::get_path(agent);
    if !path.exists() {
        return Err(format_error_message(
            AGENT_NOT_FOUND_ERROR,
            path.as_path().to_string_lossy(),
        ));
    }
    Ok(())
}
