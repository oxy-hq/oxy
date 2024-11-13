use executor::WorkflowExecutor;

use crate::yaml_parsers::config_parser::{get_config_path, parse_config};

pub mod executor;

pub async fn run_workflow(workflow_name: &str) -> anyhow::Result<String> {
    let config_path = get_config_path();
    let config = parse_config(&config_path)?;
    let workflow = config.load_workflow(workflow_name)?;
    let mut executor = WorkflowExecutor::default();
    executor.init(&config).await?;
    let response = executor.execute(&workflow).await?;
    Ok(response)
}
