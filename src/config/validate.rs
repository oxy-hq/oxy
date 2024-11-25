use super::model::Config;
use crate::ai::retrieval::{embedding_model_from_str, rerank_model_from_str};
use std::{env, fmt::Display, path::PathBuf, rc::Rc};

const INVALID_EMBED_MODEL_ERROR: &str = "invalid embedding model";
const INVALID_RERANK_MODEL_ERROR: &str = "invalid reranking model";
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

    if !path.exists() {
        return Err(format_error_message(
            FILE_NOT_FOUND_ERROR,
            path.as_path().to_string_lossy(),
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

pub fn validate_embed_model(embed_model: &str, _: &ValidationContext) -> garde::Result {
    let result = embedding_model_from_str(embed_model);
    match result {
        Ok(_) => Ok(()),
        Err(_) => Err(format_error_message(INVALID_EMBED_MODEL_ERROR, embed_model)),
    }
}

pub fn validate_rerank_model(rerank_model: &str, _: &ValidationContext) -> garde::Result {
    let result = rerank_model_from_str(rerank_model);
    match result {
        Ok(_) => Ok(()),
        Err(_) => Err(format_error_message(
            INVALID_RERANK_MODEL_ERROR,
            rerank_model,
        )),
    }
}

pub struct ValidationContext {
    pub config: Rc<Config>,
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

pub fn validate_sql_file(sql_file: &str, context: &ValidationContext) -> garde::Result {
    let path = context.config.get_sql_dir().join(sql_file);
    if !path.exists() {
        return Err(format_error_message(
            SQL_FILE_NOT_FOUND_ERROR,
            path.as_path().to_string_lossy(),
        ));
    }
    Ok(())
}

pub fn validate_agent_exists(agent: &str, context: &ValidationContext) -> garde::Result {
    let path = context
        .config
        .get_agents_dir()
        .join(format!("{}.yml", agent));
    if !path.exists() {
        return Err(format_error_message(
            AGENT_NOT_FOUND_ERROR,
            path.as_path().to_string_lossy(),
        ));
    }
    Ok(())
}
