use super::config::get_app_tasks;
use super::data::{clean_up_app_data, ensure_data_directory, get_app_data_path, save_data_to_file};
use super::types::AppResult;
use crate::execute::types::DataContainer;
use crate::project::resolve_project_path;
use crate::service::workflow::WorkflowEventHandler;
use crate::workflow::WorkflowLauncher;
use crate::workflow::loggers::NoopLogger;
use std::path::PathBuf;

pub async fn run_app(app_file_relative_path: &PathBuf) -> AppResult<DataContainer> {
    tracing::info!("Running app: {app_file_relative_path:?}");

    let app_tasks = get_app_tasks(app_file_relative_path).await?;
    let (data_path, data_file_path) = get_app_data_path(app_file_relative_path, &app_tasks)?;
    let project_path = resolve_project_path()?;

    clean_up_app_data(app_file_relative_path, &app_tasks).await?;

    let output_container = WorkflowLauncher::new()
        .with_local_context(&project_path)
        .await?
        .launch_tasks(app_tasks, WorkflowEventHandler::new(NoopLogger {}))
        .await?;

    ensure_data_directory(&data_path)?;

    let data = output_container.to_data(&data_path)?;
    save_data_to_file(&data, &data_file_path)?;

    Ok(data)
}
