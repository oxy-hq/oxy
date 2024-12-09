use std::path::PathBuf;

use context::Output;
use executor::WorkflowExecutor;

use crate::config::{get_config_path, parse_config};

pub mod context;
pub mod executor;
pub mod table;

pub async fn run_workflow(workflow_path: &PathBuf) -> anyhow::Result<Output> {
    let config_path = get_config_path();
    let config = parse_config(&config_path)?;
    let workflow = config.load_workflow(workflow_path)?;
    config.validate_workflow(&workflow)?;
    let mut executor = WorkflowExecutor::default();
    executor.init(&config).await?;
    let response = executor.execute(&workflow).await?;
    Ok(response)
}
