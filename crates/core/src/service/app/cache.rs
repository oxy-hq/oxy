use super::types::{APP_DATA_EXTENSION, APP_FILE_EXTENSION, AppResult, DATA_DIR_NAME};
use crate::config::ConfigManager;
use crate::config::model::Task;
use crate::errors::OxyError;
use crate::execute::types::{DataContainer, OutputContainer};
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use xxhash_rust::xxh3::xxh3_64;

pub struct AppCache {
    config_manager: ConfigManager,
}

impl AppCache {
    pub fn new(config_manager: ConfigManager) -> Self {
        Self { config_manager }
    }

    pub async fn clean_up_data(&self, app_path: &PathBuf, tasks: &[Task]) -> AppResult<()> {
        let (data_path, _) = self.get_data_paths(app_path, tasks)?;
        let state_dir = self.config_manager.resolve_state_dir().await?;
        let data_path = state_dir.join(data_path);
        if data_path.exists() {
            std::fs::remove_dir_all(&data_path).map_err(|e| {
                OxyError::RuntimeError(format!("Failed to remove data directory: {e}"))
            })?;
        }
        Ok(())
    }

    pub async fn try_load_data(&self, app_path: &PathBuf, tasks: &[Task]) -> Option<DataContainer> {
        let (_data_path, data_file_path) = self.get_data_paths(app_path, tasks).ok()?;

        let state_dir = self
            .config_manager
            .resolve_state_dir()
            .await
            .map_err(|e| {
                tracing::warn!("Failed to resolve state directory: {}", e);
            })
            .ok()?;

        let full_cache_path = state_dir.join(data_file_path);

        if !full_cache_path.exists() {
            return None;
        }

        self.load_from_file(&full_cache_path)
    }

    pub async fn save_data(
        &self,
        app_path: &PathBuf,
        tasks: &[Task],
        output_container: OutputContainer,
    ) -> AppResult<DataContainer> {
        let (data_path, data_file_path) = self.get_data_paths(app_path, tasks)?;
        let state_dir = self.config_manager.resolve_state_dir().await?;

        let full_data_path = state_dir.join(&data_path);
        self.ensure_directory(&full_data_path)?;

        let data = output_container.to_data(&data_path, &state_dir)?;

        let full_cache_path = state_dir.join(&data_file_path);

        self.save_to_file(&data, &full_cache_path)?;

        Ok(data)
    }

    fn get_data_paths(&self, app_path: &PathBuf, tasks: &[Task]) -> AppResult<(PathBuf, PathBuf)> {
        tracing::debug!("Getting app data path: {app_path:?}");
        let full_path = app_path;

        let file_name = full_path
            .file_name()
            .ok_or_else(|| OxyError::ConfigurationError("Invalid file path".to_string()))?
            .to_string_lossy()
            .to_string();

        if !file_name.ends_with(APP_FILE_EXTENSION) {
            return Err(OxyError::ConfigurationError(format!(
                "File must have {APP_FILE_EXTENSION} extension"
            )));
        }

        let tasks_hash = self.generate_task_hash(tasks)?;
        let data_file_name = format!(
            "{}.{}",
            tasks_hash,
            file_name.replace(APP_FILE_EXTENSION, APP_DATA_EXTENSION)
        );

        let directory_name = file_name.replace(APP_FILE_EXTENSION, "");
        let data_path: PathBuf = full_path
            .parent()
            .ok_or_else(|| OxyError::RuntimeError("Invalid file path structure".to_string()))?
            .join(DATA_DIR_NAME)
            .join(directory_name);
        let data_file_path = data_path.join(data_file_name);

        Ok((data_path, data_file_path))
    }

    fn generate_task_hash(&self, tasks: &[Task]) -> AppResult<String> {
        let tasks_serialized = serde_json::to_string(tasks)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to serialize tasks: {e}")))?;
        let tasks_hash = xxh3_64(tasks_serialized.as_bytes());
        Ok(format!("{tasks_hash:x}"))
    }

    fn ensure_directory(&self, data_path: &PathBuf) -> AppResult<()> {
        if !data_path.exists() {
            std::fs::create_dir_all(data_path).map_err(|e| {
                OxyError::RuntimeError(format!("Failed to create data directory: {e}"))
            })?;
        }
        Ok(())
    }

    fn save_to_file(&self, data: &DataContainer, file_path: &PathBuf) -> AppResult<()> {
        let data_file = std::fs::File::create(file_path)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to create data file: {e}")))?;
        let writer = BufWriter::new(data_file);
        serde_yaml::to_writer(writer, data)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to write data to file: {e}")))?;
        Ok(())
    }

    fn load_from_file(&self, file_path: &PathBuf) -> Option<DataContainer> {
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
}
