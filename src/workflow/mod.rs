use std::path::PathBuf;

use context::Output;
use executor::WorkflowExecutor;
use pyo3::prelude::*;

use crate::config::{get_config_path, parse_config};

pub mod context;
pub mod executor;
pub mod table;

#[pyclass(module = "onyx_py")]
#[derive(Debug, Clone)]
pub struct WorkflowResultStep {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub output: String,
}

#[pyclass(module = "onyx_py")]
pub struct WorkflowResult {
    #[pyo3(get)]
    pub output: Output,
    #[pyo3(get)]
    pub steps: Vec<WorkflowResultStep>,
}

pub async fn run_workflow(workflow_path: &PathBuf) -> anyhow::Result<WorkflowResult> {
    let config_path = get_config_path();
    let config = parse_config(&config_path)?;
    let workflow = config.load_workflow(workflow_path)?;
    config.validate_workflow(&workflow)?;
    let mut executor = WorkflowExecutor::default();
    executor.init(&config).await?;
    let result = executor.execute(&workflow).await?;
    Ok(result)
}
