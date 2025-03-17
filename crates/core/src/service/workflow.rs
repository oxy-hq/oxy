use base64::{Engine, prelude::BASE64_STANDARD};
use minijinja::Value;
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};

use crate::{
    config::{ConfigBuilder, model::Workflow},
    errors::OxyError,
    execute::{
        core::{event::Dispatcher, run},
        workflow::{LogItem, WorkflowExporter, WorkflowInput, WorkflowLogger, WorkflowReceiver},
    },
    utils::find_project_path,
    workflow::executor::WorkflowExecutor,
};

#[derive(Serialize)]
pub struct WorkflowInfo {
    pub name: String,
    pub path: String,
}

pub async fn list_workflows() -> Result<Vec<WorkflowInfo>, OxyError> {
    let project_path = find_project_path()?;
    let config = ConfigBuilder::new()
        .with_project_path(project_path.clone())?
        .build()
        .await?;

    let workflow_paths = config.list_workflows().await?;
    let mut workflows = Vec::new();

    for path in workflow_paths {
        if let Some(name) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_suffix(".workflow"))
        {
            workflows.push(WorkflowInfo {
                name: name.to_string(),
                path: path
                    .strip_prefix(project_path.clone())
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string(),
            });
        }
    }

    Ok(workflows)
}

pub async fn get_workflow(relative_path: PathBuf) -> Result<Workflow, OxyError> {
    let project_path = find_project_path()?;
    let config = ConfigBuilder::new()
        .with_project_path(project_path.clone())?
        .build()
        .await?;

    let full_workflow_path = project_path.join(&relative_path);
    let workflow = config.resolve_workflow(&full_workflow_path).await?;

    Ok(workflow)
}

pub async fn run_workflow(path: &PathBuf, logger: Box<dyn WorkflowLogger>) -> Result<(), OxyError> {
    let project_path = find_project_path()?;

    let config = ConfigBuilder::new()
        .with_project_path(project_path)
        .unwrap()
        .build()
        .await?;
    let workflow = config.resolve_workflow(&path).await?;

    // Create a channel to send logs to the client
    let dispatcher = Dispatcher::new(vec![
        Box::new(WorkflowReceiver::new(logger)),
        Box::new(WorkflowExporter),
    ]);
    let executor = WorkflowExecutor::new(workflow.clone());
    let ctx = Value::from_serialize(&workflow.variables);
    tokio::spawn(async move {
        run(
            &executor,
            WorkflowInput,
            Arc::new(config),
            ctx,
            Some(&workflow),
            dispatcher,
        )
        .await
        .unwrap();
    });
    Ok(())
}

pub async fn get_workflow_logs(path: &PathBuf) -> Result<Vec<LogItem>, OxyError> {
    let project_path = find_project_path()?;
    let full_workflow_path = project_path.join(path);
    let full_workflow_path_b64: String =
        BASE64_STANDARD.encode(full_workflow_path.to_str().unwrap());
    let log_file_path = format!("/var/tmp/oxy-{}.log.json", full_workflow_path_b64);
    let content = std::fs::read_to_string(log_file_path);
    match content {
        Ok(content) => {
            let mut logs: Vec<LogItem> = vec![];
            let lines = content.lines();
            for line in lines {
                let log_item: LogItem = serde_json::from_str(line).unwrap();
                logs.push(log_item);
            }
            Ok(logs)
        }
        Err(_) => Ok(vec![]),
    }
}
