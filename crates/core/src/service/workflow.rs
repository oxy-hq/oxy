use base64::{Engine, prelude::BASE64_STANDARD};
use serde::Serialize;
use slugify::slugify;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use utoipa::ToSchema;

use super::eval::PBarsHandler;
use crate::{
    adapters::{project::manager::ProjectManager, session_filters::SessionFilters},
    config::{
        ConfigManager,
        constants::{CONCURRENCY_SOURCE, CONSISTENCY_SOURCE, TASK_SOURCE, WORKFLOW_SOURCE},
        model::{ExecuteSQLTask, SQL, Task, TaskType, Workflow},
    },
    constants::{WORKFLOW_FILE_EXTENSION, WORKFLOW_SAVED_FROM_QUERY_DIR},
    errors::OxyError,
    execute::{
        types::{Event, EventKind, Output, OutputContainer, ProgressType},
        writer::EventHandler,
    },
    workflow::{
        RetryStrategy, WorkflowInput, WorkflowLauncher,
        loggers::types::{LogItem, WorkflowLogger},
    },
};

#[derive(Serialize, ToSchema)]
pub struct WorkflowInfo {
    pub name: String,
    pub path: String,
}

pub async fn list_workflows(config_manager: ConfigManager) -> Result<Vec<WorkflowInfo>, OxyError> {
    let project_path = config_manager.project_path();

    let workflow_paths = config_manager.list_workflows().await?;
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
                    .strip_prefix(project_path)
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
    config_manager: ConfigManager,
) -> Result<Workflow, OxyError> {
    let workflow = config_manager.resolve_workflow(&relative_path).await?;

    Ok(workflow)
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
        tracing::debug!(?event, "Received event");
        match event.source.kind.as_str() {
            WORKFLOW_SOURCE => match event.kind {
                EventKind::Started { name, .. } => {
                    self.logger.log(&format!("\n\n⏳Running workflow: {name}"));
                }
                EventKind::Finished { message, .. } => {
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
                EventKind::Error { message } => {
                    self.logger.log_error(&message);
                }
                _ => {}
            },
            CONCURRENCY_SOURCE => {}
            _ => match event.kind {
                EventKind::Started { name, .. } => {
                    if event.source.kind.as_str() == TASK_SOURCE {
                        self.logger.log(&format!("\n⏳Starting {name}"));
                    }
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
                EventKind::Error { message } => {
                    self.logger.log_error(&message);
                }
                _ => {}
            },
        }
        Ok(())
    }
}

pub async fn run_workflow<P: AsRef<Path>, L: WorkflowLogger + 'static>(
    path: P,
    logger: L,
    retry_strategy: RetryStrategy,
    variables: Option<HashMap<String, serde_json::Value>>,
    project_manager: ProjectManager,
    filters: Option<SessionFilters>,
) -> Result<OutputContainer, OxyError> {
    WorkflowLauncher::new()
        .with_filters(filters)
        .with_project(project_manager)
        .await?
        .launch(
            WorkflowInput {
                workflow_ref: path.as_ref().to_string_lossy().to_string(),
                variables,
                retry: retry_strategy,
            },
            WorkflowEventHandler::new(logger),
        )
        .await
}

pub async fn run_workflow_v2<P: AsRef<Path>, H: EventHandler + Send + Sync + 'static>(
    project_manager: ProjectManager,
    path: P,
    handler: H,
    retry_strategy: RetryStrategy,
    variables: Option<HashMap<String, serde_json::Value>>,
    filters: Option<SessionFilters>,
) -> Result<OutputContainer, OxyError> {
    WorkflowLauncher::new()
        .with_filters(filters)
        .with_project(project_manager)
        .await?
        .launch(
            WorkflowInput {
                workflow_ref: path.as_ref().to_string_lossy().to_string(),
                variables,
                retry: retry_strategy,
            },
            handler,
        )
        .await
}

pub async fn get_workflow_logs(
    path: &PathBuf,
    config_manager: ConfigManager,
) -> Result<Vec<LogItem>, OxyError> {
    let full_workflow_path = config_manager.resolve_file(path).await?;
    let full_workflow_path_b64: String = BASE64_STANDARD.encode(full_workflow_path);
    let log_file_path = format!("/var/tmp/oxy-{full_workflow_path_b64}.log.json");
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

pub async fn create_workflow_from_query(
    query: &str,
    prompt: &str,
    database: &str,
    config_manager: &ConfigManager,
) -> Result<Workflow, OxyError> {
    let task = Task {
        task_type: TaskType::ExecuteSQL(ExecuteSQLTask {
            sql: SQL::Query {
                sql_query: query.to_string(),
            },
            database: database.to_string(),
            export: None,
            dry_run_limit: None,
            variables: None,
        }),
        cache: None,
        name: "execute_sql".to_string(),
    };
    let workflow_name = slugify!(prompt, separator = "_");
    let workflow = Workflow {
        name: workflow_name.clone(),
        description: prompt.to_string(),
        tasks: vec![task],
        tests: vec![],
        variables: None,
        retrieval: Default::default(),
    };
    // write workflow to file
    let workflow_dir = config_manager
        .resolve_file(WORKFLOW_SAVED_FROM_QUERY_DIR)
        .await?;
    let workflow_dir = PathBuf::from(workflow_dir);
    if !workflow_dir.exists() {
        std::fs::create_dir_all(&workflow_dir)?;
    }
    let workflow_path = workflow_dir.join(format!("{}{}", &workflow_name, WORKFLOW_FILE_EXTENSION));

    let _ = serde_yaml::to_writer(std::fs::File::create(&workflow_path)?, &workflow);

    Ok(workflow)
}
