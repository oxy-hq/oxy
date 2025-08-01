use super::types::{APP_DATA_EXTENSION, APP_FILE_EXTENSION, AppResult, DATA_DIR_NAME};
use super::utils::{generate_task_hash, map_runtime_error};
use crate::config::model::Task;
use crate::db::client::get_state_dir;
use crate::execute::types::DataContainer;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

pub async fn clean_up_app_data(app_file_relative_path: &PathBuf, tasks: &[Task]) -> AppResult<()> {
    let (data_path, _) = get_app_data_path(app_file_relative_path, tasks)?;
    if data_path.exists() {
        std::fs::remove_dir_all(&data_path)
            .map_err(map_runtime_error("Failed to remove data directory"))?;
    }
    Ok(())
}

pub fn try_load_cached_data(app_file_path: &PathBuf, tasks: &[Task]) -> Option<DataContainer> {
    let (data_path, data_file_path) = get_app_data_path(app_file_path, tasks).ok()?;

    if !data_path.exists() {
        return None;
    }

    load_data_from_file(&data_file_path)
}

pub fn get_app_data_path(
    app_file_relative_path: &PathBuf,
    tasks: &[Task],
) -> AppResult<(PathBuf, PathBuf)> {
    tracing::debug!("Getting app data path: {app_file_relative_path:?}");

    let state_dir = get_state_dir();
    let full_path = state_dir.join(app_file_relative_path);

    let file_name = full_path
        .file_name()
        .ok_or_else(|| {
            crate::errors::OxyError::ConfigurationError("Invalid file path".to_string())
        })?
        .to_string_lossy()
        .to_string();

    if !file_name.ends_with(APP_FILE_EXTENSION) {
        return Err(crate::errors::OxyError::ConfigurationError(format!(
            "File must have {} extension",
            APP_FILE_EXTENSION
        )));
    }

    let tasks_hash = generate_task_hash(tasks)?;
    let data_file_name = format!(
        "{}.{}",
        tasks_hash,
        file_name.replace(APP_FILE_EXTENSION, APP_DATA_EXTENSION)
    );

    let directory_name = file_name.replace(APP_FILE_EXTENSION, "");
    let data_path: PathBuf = full_path
        .parent()
        .ok_or_else(|| {
            crate::errors::OxyError::RuntimeError("Invalid file path structure".to_string())
        })?
        .join(DATA_DIR_NAME)
        .join(directory_name);
    let data_file_path = data_path.join(data_file_name);

    Ok((data_path, data_file_path))
}

pub fn ensure_data_directory(data_path: &PathBuf) -> AppResult<()> {
    if !data_path.exists() {
        std::fs::create_dir_all(data_path)
            .map_err(map_runtime_error("Failed to create data directory"))?;
    }
    Ok(())
}

pub fn save_data_to_file(data: &DataContainer, file_path: &PathBuf) -> AppResult<()> {
    let data_file = std::fs::File::create(file_path)
        .map_err(map_runtime_error("Failed to create data file"))?;
    let writer = BufWriter::new(data_file);
    serde_yaml::to_writer(writer, data)
        .map_err(map_runtime_error("Failed to write data to file"))?;
    Ok(())
}

pub fn load_data_from_file(file_path: &PathBuf) -> Option<DataContainer> {
    let file = std::fs::File::open(file_path).ok()?;
    let reader = BufReader::new(file);
    match serde_yaml::from_reader(reader) {
        Ok(data) => Some(data),
        Err(e) => {
            tracing::warn!("Failed to parse data file: {}", e);
            None
        }
    }
}
