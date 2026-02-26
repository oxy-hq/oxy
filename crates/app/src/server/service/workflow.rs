use base64::{Engine, prelude::BASE64_STANDARD};
use serde::Serialize;
use slugify::slugify;
use std::path::{Path, PathBuf};
use utoipa::ToSchema;

use super::eval::PBarsHandler;

use oxy::{
    adapters::{project::manager::ProjectManager, session_filters::SessionFilters},
    checkpoint::types::RetryStrategy,
    config::{
        ConfigManager,
        constants::{CONCURRENCY_SOURCE, CONSISTENCY_SOURCE, TASK_SOURCE, WORKFLOW_SOURCE},
        model::{
            ConnectionOverrides, ExecuteSQLTask, RouteRetrievalConfig, SQL, Task, TaskType,
            Workflow,
        },
    },
    constants::{
        AUTOMATION_FILE_EXTENSION, AUTOMATION_SAVED_DIR, WORKFLOW_FILE_EXTENSION,
        WORKFLOW_SAVED_FROM_QUERY_DIR,
    },
    execute::{
        types::{Event, EventKind, Output, OutputContainer, ProgressType},
        writer::EventHandler,
    },
    observability::events::workflow as workflow_events,
};
use oxy_shared::errors::OxyError;
use oxy_workflow::{
    WorkflowInput, WorkflowLauncher,
    loggers::types::{LogItem, WorkflowLogger},
};

#[derive(Serialize, ToSchema)]
pub struct WorkflowInfo {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

pub async fn list_workflows(config_manager: ConfigManager) -> Result<Vec<WorkflowInfo>, OxyError> {
    let project_path = config_manager.project_path();

    let workflow_paths = config_manager.list_workflows().await?;
    let mut workflows = Vec::new();

    for path in workflow_paths {
        if let Some(name) = path.file_stem().and_then(|s| s.to_str()).and_then(|s| {
            s.strip_suffix(".workflow")
                .or_else(|| s.strip_suffix(".automation"))
        }) {
            let relative_path_str = path
                .strip_prefix(project_path)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();

            workflows.push(WorkflowInfo {
                name: name.to_string(),
                path: relative_path_str,
                tasks: None,
                description: None,
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
                    Output::SemanticQuery(semantic_query) => {
                        let json = serde_json::to_string_pretty(&semantic_query)
                            .unwrap_or_else(|_| "Failed to serialize SemanticQuery".to_string());
                        self.logger
                            .log(&format!("Semantic Query:\n```json\n{json}\n```"));
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

#[tracing::instrument(skip_all, err, fields(
    otel.name = workflow_events::run_workflow::NAME,
    oxy.span_type = workflow_events::run_workflow::TYPE,
    oxy.workflow.ref = %path.as_ref().to_string_lossy().to_string(),
    oxy.execution.source = tracing::field::Empty,
    oxy.user.id = tracing::field::Empty,
    oxy.thread.id = tracing::field::Empty,
    oxy.task.id = tracing::field::Empty,
    oxy.context.id = tracing::field::Empty,
))]
pub async fn run_workflow<P: AsRef<Path>, L: WorkflowLogger + 'static>(
    path: P,
    logger: L,
    retry_strategy: RetryStrategy,
    project_manager: ProjectManager,
    filters: Option<SessionFilters>,
    connections: Option<ConnectionOverrides>,
    globals: Option<indexmap::IndexMap<String, serde_json::Value>>,
    source: Option<crate::service::agent::ExecutionSource>,
    user_id: Option<uuid::Uuid>,
) -> Result<OutputContainer, OxyError> {
    workflow_events::run_workflow::input(
        &path.as_ref().to_string_lossy(),
        &format!("{:?}", retry_strategy),
    );

    // Record execution source in tracing span
    if let Some(ref exec_source) = source {
        let span = tracing::Span::current();
        span.record(
            "oxy.execution.source",
            format!("{:?}", exec_source).as_str(),
        );

        match exec_source {
            crate::service::agent::ExecutionSource::WebApi { thread_id, user_id } => {
                span.record("oxy.user.id", user_id.as_str());
                span.record("oxy.thread.id", thread_id.as_str());
            }
            crate::service::agent::ExecutionSource::Slack {
                thread_id,
                channel_id,
            } => {
                span.record("oxy.thread.id", thread_id.as_str());
                if let Some(cid) = channel_id {
                    span.record("oxy.context.id", cid.as_str());
                }
            }
            crate::service::agent::ExecutionSource::A2a {
                task_id,
                context_id,
                thread_id,
            } => {
                span.record("oxy.task.id", task_id.as_str());
                span.record("oxy.context.id", context_id.as_str());
                span.record("oxy.thread.id", thread_id.as_str());
            }
            crate::service::agent::ExecutionSource::Mcp { session_id } => {
                if let Some(sid) = session_id {
                    span.record("oxy.context.id", sid.as_str());
                }
            }
            _ => {}
        }
    }
    let result = WorkflowLauncher::new()
        .with_filters(filters)
        .with_connections(connections)
        .with_globals(globals)
        .with_project(project_manager)
        .await?
        .launch(
            WorkflowInput {
                workflow_ref: path.as_ref().to_string_lossy().to_string(),
                retry: retry_strategy,
            },
            WorkflowEventHandler::new(logger),
            user_id,
        )
        .await;

    match &result {
        Ok(output) => workflow_events::run_workflow::output(output),
        Err(e) => workflow_events::run_workflow::error(&e.to_string()),
    }

    result
}

#[tracing::instrument(skip_all, err, fields(
    otel.name = workflow_events::run_workflow::NAME,
    oxy.span_type = workflow_events::run_workflow::TYPE,
    oxy.workflow.ref = %path.as_ref().to_string_lossy().to_string(),
    oxy.execution.source = tracing::field::Empty,
    oxy.user.id = tracing::field::Empty,
    oxy.thread.id = tracing::field::Empty,
    oxy.task.id = tracing::field::Empty,
    oxy.context.id = tracing::field::Empty,
))]
pub async fn run_workflow_v2<P: AsRef<Path>, H: EventHandler + Send + Sync + 'static>(
    project_manager: ProjectManager,
    path: P,
    handler: H,
    retry_strategy: RetryStrategy,
    filters: Option<SessionFilters>,
    connections: Option<ConnectionOverrides>,
    globals: Option<indexmap::IndexMap<String, serde_json::Value>>,
    source: Option<crate::service::agent::ExecutionSource>,
    user_id: Option<uuid::Uuid>,
) -> Result<OutputContainer, OxyError> {
    workflow_events::run_workflow::input(
        &path.as_ref().to_string_lossy(),
        &format!("{:?}", retry_strategy),
    );

    // Record execution source in tracing span
    if let Some(ref exec_source) = source {
        let span = tracing::Span::current();
        span.record(
            "oxy.execution.source",
            format!("{:?}", exec_source).as_str(),
        );

        match exec_source {
            crate::service::agent::ExecutionSource::WebApi { thread_id, user_id } => {
                span.record("oxy.user.id", user_id.as_str());
                span.record("oxy.thread.id", thread_id.as_str());
            }
            crate::service::agent::ExecutionSource::Slack {
                thread_id,
                channel_id,
            } => {
                span.record("oxy.thread.id", thread_id.as_str());
                if let Some(cid) = channel_id {
                    span.record("oxy.context.id", cid.as_str());
                }
            }
            crate::service::agent::ExecutionSource::A2a {
                task_id,
                context_id,
                thread_id,
            } => {
                span.record("oxy.task.id", task_id.as_str());
                span.record("oxy.context.id", context_id.as_str());
                span.record("oxy.thread.id", thread_id.as_str());
            }
            crate::service::agent::ExecutionSource::Mcp { session_id } => {
                if let Some(sid) = session_id {
                    span.record("oxy.context.id", sid.as_str());
                }
            }
            _ => {}
        }
    }
    let result = WorkflowLauncher::new()
        .with_filters(filters)
        .with_connections(connections)
        .with_globals(globals)
        .with_project(project_manager)
        .await?
        .launch(
            WorkflowInput {
                workflow_ref: path.as_ref().to_string_lossy().to_string(),
                retry: retry_strategy,
            },
            handler,
            user_id,
        )
        .await;

    match &result {
        Ok(output) => workflow_events::run_workflow::output(output),
        Err(e) => workflow_events::run_workflow::error(&e.to_string()),
    }

    result
}

pub async fn get_workflow_logs(
    path: &PathBuf,
    config_manager: ConfigManager,
) -> Result<Vec<LogItem>, OxyError> {
    let full_workflow_path = config_manager.resolve_file(path).await?;
    let full_workflow_path_b64: String = BASE64_STANDARD.encode(full_workflow_path);
    let log_file_path = std::env::temp_dir()
        .join(format!("oxy-{full_workflow_path_b64}.log.json"))
        .to_string_lossy()
        .into_owned();
    let content = std::fs::read_to_string(log_file_path);
    match content {
        Ok(content) => {
            let mut logs: Vec<LogItem> = vec![];
            let lines = content.lines();
            for line in lines {
                match serde_json::from_str::<LogItem>(line) {
                    Ok(log_item) => logs.push(log_item),
                    Err(e) => tracing::warn!("Skipping malformed log line: {}", e),
                }
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
        consistency_prompt: None,
    };
    // write workflow to file
    let workflow_dir = config_manager
        .resolve_file(WORKFLOW_SAVED_FROM_QUERY_DIR)
        .await?;
    let workflow_dir = PathBuf::from(workflow_dir);
    tokio::fs::create_dir_all(&workflow_dir).await?;
    let workflow_path = workflow_dir.join(format!("{}{}", &workflow_name, WORKFLOW_FILE_EXTENSION));

    let yaml = serde_yaml::to_string(&workflow)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to serialize workflow: {}", e)))?;
    tokio::fs::write(&workflow_path, yaml).await?;

    Ok(workflow)
}

pub async fn create_automation(
    name: &str,
    description: &str,
    tasks: Vec<Task>,
    retrieval: Option<RouteRetrievalConfig>,
    config_manager: &ConfigManager,
) -> Result<(Workflow, String), OxyError> {
    let automation_name = {
        let slug = slugify!(name, separator = "_");
        if slug.is_empty() {
            return Err(OxyError::ArgumentError(format!(
                "Automation name {:?} produces an empty slug; please use alphanumeric characters.",
                name
            )));
        }
        slug
    };

    if tasks.is_empty() {
        return Err(OxyError::ArgumentError(
            "Cannot save an automation with no executable tasks.".to_string(),
        ));
    }

    let workflow = Workflow {
        name: automation_name.clone(),
        description: description.to_string(),
        tasks,
        tests: vec![],
        variables: None,
        retrieval,
        consistency_prompt: None,
    };

    let automation_dir = config_manager.resolve_file(AUTOMATION_SAVED_DIR).await?;
    let automation_dir = PathBuf::from(automation_dir);
    tokio::fs::create_dir_all(&automation_dir)
        .await
        .map_err(|e| {
            OxyError::RuntimeError(format!("Failed to create automation directory: {}", e))
        })?;

    let yaml = serde_yaml::to_string(&workflow)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to serialize automation: {}", e)))?;

    // Use create_new(true) to atomically find a unique path and write the file,
    // avoiding both blocking exists() calls and the TOCTOU race.
    let mut candidate_name = automation_name.clone();
    let mut counter = 2u32;
    let automation_path = loop {
        let path = automation_dir.join(format!("{}{}", candidate_name, AUTOMATION_FILE_EXTENSION));
        match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await
        {
            Ok(mut file) => {
                use tokio::io::AsyncWriteExt;
                file.write_all(yaml.as_bytes()).await.map_err(|e| {
                    OxyError::RuntimeError(format!("Failed to write automation file: {}", e))
                })?;
                break path;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                candidate_name = format!("{}_{}", automation_name, counter);
                counter += 1;
            }
            Err(e) => {
                return Err(OxyError::RuntimeError(format!(
                    "Failed to create automation file: {}",
                    e
                )));
            }
        }
    };
    let relative_path = Path::new(AUTOMATION_SAVED_DIR)
        .join(format!("{}{}", candidate_name, AUTOMATION_FILE_EXTENSION));

    tracing::info!("Saved automation to: {}", automation_path.display());
    Ok((workflow, relative_path.to_string_lossy().to_string()))
}
