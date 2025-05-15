use std::path::PathBuf;

use crate::config::ConfigBuilder;
use crate::config::model::AppConfig;
use crate::db::client::get_state_dir;
use crate::errors::OxyError;
use crate::execute::types::DataContainer;
use crate::service::workflow::WorkflowEventHandler;
use crate::utils::find_project_path;
use crate::workflow::WorkflowLauncher;
use crate::workflow::loggers::NoopLogger;

pub async fn get_app(path: &PathBuf) -> Result<AppConfig, OxyError> {
    let config_builder = ConfigBuilder::new().with_project_path(find_project_path()?)?;
    let config = config_builder.build().await?;
    let app = config.resolve_app(path).await?;
    Ok(app)
}

pub async fn run_app(app_file_relative_path: &PathBuf) -> Result<DataContainer, OxyError> {
    println!("Running app: {:?}", app_file_relative_path);
    let (data_path, data_file_path) = get_app_data_path(app_file_relative_path)?;
    let project_path = find_project_path()?;

    let app_config = get_app(app_file_relative_path).await?;

    // clean up the data directory
    clean_up_app_data(app_file_relative_path).await?;

    let rs = WorkflowLauncher::new()
        .with_local_context(&project_path)
        .await?
        .launch_tasks(app_config.tasks, WorkflowEventHandler::new(NoopLogger {}))
        .await?;

    if !data_path.exists() {
        std::fs::create_dir_all(&data_path).map_err(|e| {
            OxyError::RuntimeError(format!("Failed to create data directory: {}", e))
        })?;
    }

    // write the data to the file
    let data = rs.to_data(&data_path)?;
    let data_file = std::fs::File::create(&data_file_path)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to create data file: {}", e)))?;
    let writer = std::io::BufWriter::new(data_file);
    serde_yaml::to_writer(writer, &data)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to write data to file: {}", e)))?;

    Ok(data)
}

pub async fn clean_up_app_data(app_file_relative_path: &PathBuf) -> Result<(), OxyError> {
    let (data_path, _) = get_app_data_path(app_file_relative_path)?;
    if data_path.exists() {
        std::fs::remove_dir_all(&data_path).map_err(|e| {
            OxyError::RuntimeError(format!("Failed to remove data directory: {}", e))
        })?;
    }
    Ok(())
}

pub fn try_load_cached_data(app_file_path: &PathBuf) -> Option<DataContainer> {
    if let Ok((data_path, data_file_path)) = get_app_data_path(app_file_path) {
        if !data_path.exists() {
            return None;
        }
        let data_file = std::fs::File::open(&data_file_path);
        match data_file {
            Ok(file) => {
                let reader = std::io::BufReader::new(file);
                let data: Option<DataContainer> = serde_yaml::from_reader(reader).ok();
                if data.is_none() {
                    tracing::warn!("Failed to parse data file");
                }
                data
            }
            Err(e) => {
                tracing::warn!("Failed to open data file: {}", e);
                None
            }
        }
    } else {
        tracing::warn!("Data file path is not valid");
        None
    }
}

pub fn get_app_data_path(app_file_relative_path: &PathBuf) -> Result<(PathBuf, PathBuf), OxyError> {
    print!("Getting app data path: {:?}", app_file_relative_path);
    let state_dir = get_state_dir();
    let full_path = state_dir.join(app_file_relative_path);
    let file_name = full_path.file_name().unwrap().to_string_lossy().to_string();
    let data_file_name = file_name.replace(".app.yml", ".app.data.yml");
    let directory_name = file_name.replace(".app.yml", "");
    let data_path: PathBuf = full_path
        .parent()
        .unwrap()
        .join(PathBuf::from("data"))
        .join(directory_name);
    let data_file_path = data_path.join(data_file_name);
    Ok((data_path, data_file_path))
}
