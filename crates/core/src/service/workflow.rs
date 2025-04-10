use base64::{Engine, prelude::BASE64_STANDARD};
use minijinja::Value;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::{
    config::{
        ConfigBuilder,
        constants::{CONCURRENCY_SOURCE, CONSISTENCY_SOURCE, WORKFLOW_SOURCE},
        model::Workflow,
    },
    errors::OxyError,
    execute::{
        core::{event::Dispatcher, run},
        types::{Event, EventKind, Output, ProgressType},
        workflow::{LogItem, WorkflowExporter, WorkflowInput, WorkflowLogger, WorkflowReceiver},
        writer::EventHandler,
    },
    utils::find_project_path,
    workflow::{WorkflowInput as NewWorkflowInput, WorkflowLauncher, executor::WorkflowExecutor},
};

use super::eval::PBarsHandler;

#[derive(Serialize)]
pub struct WorkflowInfo {
    pub name: String,
    pub path: String,
}

pub async fn list_workflows(project_path: Option<PathBuf>) -> Result<Vec<WorkflowInfo>, OxyError> {
    let project_path = match project_path {
        Some(path) => path,
        None => find_project_path()?,
    };
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

pub async fn get_workflow(
    relative_path: PathBuf,
    project_path: Option<PathBuf>,
) -> Result<Workflow, OxyError> {
    let project_path = match project_path {
        Some(path) => path,
        None => find_project_path()?,
    };

    let config = ConfigBuilder::new()
        .with_project_path(project_path.clone())?
        .build()
        .await?;

    let full_workflow_path = project_path.join(&relative_path);
    let workflow = config.resolve_workflow(&full_workflow_path).await?;

    Ok(workflow)
}

pub async fn run_workflow<P: AsRef<Path>, L: WorkflowLogger + 'static>(
    path: P,
    logger: L,
    restore_from_checkpoint: bool,
) -> Result<(), OxyError> {
    #[cfg(feature = "builders")]
    return run_workflow_with_builder(path, logger, restore_from_checkpoint).await;
    #[cfg(not(feature = "builders"))]
    return run_workflow_legacy(path, logger, restore_from_checkpoint).await;
}

async fn run_workflow_legacy<P: AsRef<Path>, L: WorkflowLogger + 'static>(
    path: P,
    logger: L,
    _restore_from_checkpoint: bool,
) -> Result<(), OxyError> {
    let project_path = find_project_path()?;

    let config = ConfigBuilder::new()
        .with_project_path(project_path)
        .unwrap()
        .build()
        .await?;
    let workflow = config.resolve_workflow(path).await?;

    // Create a channel to send logs to the client
    let dispatcher = Dispatcher::new(vec![
        Box::new(WorkflowReceiver::new(logger)),
        Box::new(WorkflowExporter),
    ]);
    let executor = WorkflowExecutor::new(workflow.clone());
    let ctx = Value::from_serialize(&workflow.variables);
    run(
        &executor,
        WorkflowInput,
        config,
        ctx,
        Some(&workflow),
        dispatcher,
    )
    .await?;
    Ok(())
}

pub struct WorkflowEventHandler<L> {
    logger: L,
    pbar_handler: PBarsHandler,
}

impl<L> WorkflowEventHandler<L> {
    pub fn new(logger: L) -> Self {
        Self {
            logger,
            pbar_handler: PBarsHandler::new(),
        }
    }
}

#[async_trait::async_trait]
impl<L> EventHandler for WorkflowEventHandler<L>
where
    L: WorkflowLogger,
{
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        log::debug!("Received event: {:?}", event);
        match event.source.kind.as_str() {
            WORKFLOW_SOURCE => match event.kind {
                EventKind::Started { name } => {
                    self.logger
                        .log(&format!("\n\n⏳Running workflow: {}", name));
                }
                EventKind::Finished { message } => {
                    self.logger.log(&message);
                }
                _ => {}
            },
            CONSISTENCY_SOURCE => match event.kind {
                EventKind::Progress { progress } => match progress {
                    ProgressType::Started(total) => {
                        self.pbar_handler.get_or_create_bar(&event.source.id, total);
                    }
                    ProgressType::Updated(progress) => {
                        self.pbar_handler.update_bar(&event.source.id, progress)?;
                    }
                    ProgressType::Finished => {
                        self.pbar_handler.remove_bar(&event.source.id);
                    }
                },
                EventKind::Message { message } => {
                    self.logger.log(&message);
                }
                _ => {}
            },
            CONCURRENCY_SOURCE => {}
            _ => match event.kind {
                EventKind::Started { name } => {
                    self.logger.log_task_started(&name);
                }
                EventKind::Updated { chunk } => match chunk.delta.clone() {
                    Output::SQL(sql) => {
                        self.logger.log_sql_query(&sql.0);
                    }
                    Output::Table(table) => {
                        self.logger.log_table_result(table);
                    }
                    Output::Text(text) => {
                        self.logger.log_text_chunk(&text, chunk.finished);
                    }
                    _ => {}
                },
                EventKind::Message { message } => {
                    self.logger.log(&message);
                }
                _ => {}
            },
        }
        Ok(())
    }
}

async fn run_workflow_with_builder<P: AsRef<Path>, L: WorkflowLogger + 'static>(
    path: P,
    logger: L,
    restore_from_checkpoint: bool,
) -> Result<(), OxyError> {
    let project_path = find_project_path()?.to_string_lossy().to_string();
    let _ = WorkflowLauncher::new()
        .with_local_context(&project_path)
        .await?
        .launch(
            NewWorkflowInput {
                workflow_ref: path.as_ref().to_string_lossy().to_string(),
                variables: None,
                restore_from_checkpoint,
            },
            WorkflowEventHandler::new(logger),
        )
        .await?;
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
