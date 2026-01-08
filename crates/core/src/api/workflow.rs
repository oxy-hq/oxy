use base64::prelude::*;
use entity::prelude::Threads;
use futures::TryFutureExt;
use indexmap::IndexMap;
use sea_orm::ActiveValue;
use sea_orm::EntityTrait;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;
use utoipa::ToSchema;
use uuid::Uuid;

use std::{
    fs::{File, OpenOptions},
    sync::{Arc, Mutex},
};

use crate::{
    adapters::{checkpoint::types::RetryStrategy, session_filters::SessionFilters},
    api::middlewares::{project::ProjectManagerExtractor, timeout::TimeoutConfig},
    auth::extractor::AuthenticatedUserExtractor,
    config::model::{ConnectionOverrides, Workflow},
    db::client::establish_connection,
    service::{
        statics::BROADCASTER,
        thread::streaming_workflow_persister::StreamingWorkflowPersister,
        types::run::RunStatus,
        workflow as service,
        workflow::{WorkflowInfo, get_workflow, run_workflow as run_workflow_service},
    },
    utils::create_sse_stream,
    workflow::loggers::{
        api::WorkflowAPILogger,
        types::{LogItem, WorkflowLogger},
    },
};
use axum::{
    extract::{self, Path},
    http::StatusCode,
    response::{IntoResponse, sse::Sse},
};
use sea_orm::ActiveModelTrait;
use serde::Serialize;
use tokio::sync::mpsc;

// =================================================================================================
// UTILITY FUNCTIONS
// =================================================================================================

fn encode_workflow_path(path: &PathBuf) -> String {
    BASE64_STANDARD.encode(path.to_str().unwrap())
}

// =================================================================================================
// API RESPONSE TYPES
// =================================================================================================

#[derive(Serialize, ToSchema)]
pub struct GetWorkflowResponse {
    #[schema(value_type = Object)]
    pub workflow: crate::config::model::Workflow,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
}

/// List all workflows in the project
///
/// Retrieves a list of all workflow configurations available in the project.
/// Returns workflow metadata including paths, names, and configuration details.
#[utoipa::path(
    method(get),
    path = "/{project_id}/workflows",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID")
    ),
    responses(
        (status = 200, description = "Success", body = Vec<WorkflowInfo>, content_type = "application/json")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Automations"
)]
pub async fn list(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
) -> Result<impl IntoResponse, StatusCode> {
    let config_manager = project_manager.config_manager;
    match crate::service::workflow::list_workflows(config_manager.clone()).await {
        Ok(workflows) => {
            let response = serde_json::to_string(&workflows).unwrap();
            Ok((StatusCode::OK, response))
        }
        Err(_e) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Get a specific workflow configuration by path
///
/// Retrieves the complete configuration for a specific workflow using its base64-encoded path.
/// Returns workflow definition including steps, transforms, outputs, and execution settings.
#[utoipa::path(
    method(get),
    path = "/{project_id}/workflows/{pathb64}",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("pathb64" = String, Path, description = "Base64 encoded path to the workflow")
    ),
    responses(
        (status = 200, description = "Workflow details retrieved successfully", body = GetWorkflowResponse),
        (status = 400, description = "Bad request - invalid path encoding"),
        (status = 404, description = "Workflow not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Automations"
)]
pub async fn get(
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
) -> Result<extract::Json<GetWorkflowResponse>, (StatusCode, extract::Json<ErrorResponse>)> {
    let decoded_path = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        tracing::warn!("Failed to decode base64 path: {:?}", e);
        (
            StatusCode::BAD_REQUEST,
            extract::Json(ErrorResponse {
                error: format!("Invalid base64 encoding: {}", e),
            }),
        )
    })?;
    let path = String::from_utf8(decoded_path).map_err(|e| {
        tracing::warn!("Failed to convert path to UTF-8: {:?}", e);
        (
            StatusCode::BAD_REQUEST,
            extract::Json(ErrorResponse {
                error: format!("Invalid UTF-8 in path: {}", e),
            }),
        )
    })?;

    let config_manager = project_manager.config_manager;

    match get_workflow(PathBuf::from(path), config_manager.clone()).await {
        Ok(workflow) => Ok(extract::Json(GetWorkflowResponse { workflow })),
        Err(error) => {
            tracing::error!("Error retrieving workflow: {:?}", error);
            Err((
                StatusCode::BAD_REQUEST,
                extract::Json(ErrorResponse {
                    error: error.to_string(),
                }),
            ))
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct GetLogsResponse {
    logs: Vec<LogItem>,
}

/// Get execution logs for a workflow
///
/// Retrieves historical execution logs for a specific workflow. Returns detailed log entries
/// including timestamps, content, and log types for debugging and monitoring workflow runs.
#[utoipa::path(
    method(get),
    path = "/{project_id}/workflows/{pathb64}/logs",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("pathb64" = String, Path, description = "Base64 encoded path to the workflow")
    ),
    responses(
        (status = 200, description = "Workflow logs retrieved successfully", body = GetLogsResponse),
        (status = 400, description = "Bad request - invalid path encoding"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Automations"
)]
pub async fn get_logs(
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
) -> Result<extract::Json<GetLogsResponse>, StatusCode> {
    let path = PathBuf::from(
        String::from_utf8(BASE64_STANDARD.decode(pathb64).map_err(|e| {
            tracing::info!("{:?}", e);
            StatusCode::BAD_REQUEST
        })?)
        .map_err(|e| {
            tracing::info!("{:?}", e);
            StatusCode::BAD_REQUEST
        })?,
    );
    let logs = service::get_workflow_logs(&path, project_manager.config_manager).await?;
    Ok(extract::Json(GetLogsResponse { logs }))
}

pub async fn build_workflow_api_logger(
    full_workflow_path: &PathBuf,
    handler: Option<Arc<StreamingWorkflowPersister>>,
) -> (WorkflowAPILogger, mpsc::Receiver<LogItem>) {
    let full_workflow_path_b64 = encode_workflow_path(full_workflow_path);
    let (sender, receiver) = mpsc::channel(100);
    let log_file_path = format!("/var/tmp/oxy-{full_workflow_path_b64}.log.json");
    File::create(log_file_path.clone()).unwrap();
    let file = OpenOptions::new()
        .append(true)
        .open(log_file_path)
        .map_err(|e| {
            tracing::error!("{:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
        .unwrap();
    let api_logger: WorkflowAPILogger =
        WorkflowAPILogger::new(sender, Some(Arc::new(Mutex::new(file))));

    let api_logger = if let Some(handler) = handler {
        api_logger.with_streaming_persister(handler)
    } else {
        api_logger
    };
    (api_logger, receiver)
}

#[derive(Deserialize, ToSchema)]
pub struct WorkflowRetryParam {
    pub run_id: String,
    pub replay_id: Option<String>,
}

pub type GlobalOverrides = IndexMap<String, serde_json::Value>;

#[derive(Deserialize, ToSchema)]
pub struct RunWorkflowRequest {
    #[schema(value_type = Object)]
    variables: Option<IndexMap<String, serde_json::Value>>,
    retry_param: Option<WorkflowRetryParam>,

    #[serde(default)]
    pub filters: Option<SessionFilters>,

    #[serde(default)]
    #[schema(value_type = Object)]
    pub connections: Option<ConnectionOverrides>,

    #[serde(default)]
    #[schema(value_type = Object)]
    pub globals: Option<GlobalOverrides>,
}

/// Execute a workflow and stream results via Server-Sent Events
///
/// Starts a workflow execution and returns a streaming SSE connection that delivers
/// real-time execution logs and events. The workflow runs asynchronously with logs
/// being persisted to disk. Supports retry functionality via retry_param.
#[utoipa::path(
    method(post),
    path = "/{project_id}/workflows/{pathb64}/run",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("pathb64" = String, Path, description = "Base64 encoded path to the workflow")
    ),
    request_body = RunWorkflowRequest,
    responses(
        (status = 200, description = "Workflow execution started successfully - returns streaming logs", content_type = "text/event-stream"),
        (status = 400, description = "Bad request - invalid path or parameters"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Automations"
)]
pub async fn run_workflow(
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    extract::Json(request): extract::Json<RunWorkflowRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let decoded_path = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let path = PathBuf::from(String::from_utf8(decoded_path).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?);

    let full_workflow_path = project_manager
        .config_manager
        .resolve_file(path.clone())
        .map_err(|e| {
            tracing::info!("{:?}", e);
            StatusCode::BAD_REQUEST
        })
        .await?;

    let full_workflow_path = PathBuf::from(&full_workflow_path);

    let (logger, receiver) = build_workflow_api_logger(&full_workflow_path, None).await;

    let filters = request.filters;
    let connections = request.connections;
    let globals = request.globals;

    let _ = tokio::spawn(async move {
        tracing::info!("Workflow run started");
        let rs = run_workflow_service(
            path,
            logger.clone(),
            RetryStrategy::NoRetry { variables: None },
            project_manager.clone(),
            filters,
            connections,
            globals,
            None, // No authenticated user for this endpoint
        )
        .await;
        match rs {
            Ok(_) => tracing::info!("Workflow run completed successfully"),
            Err(e) => {
                tracing::error!("Workflow run failed: {:?}", e);
                logger.log_error(&format!("Workflow run failed: {e:?}"));
            }
        }
    });

    let stream = create_sse_stream(receiver);

    Ok(Sse::new(stream))
}

async fn unlock_workflow_thread(
    thread: &entity::threads::Model,
    connection: &sea_orm::DatabaseConnection,
) {
    let mut thread_model: entity::threads::ActiveModel = thread.clone().into();
    thread_model.is_processing = ActiveValue::Set(false);

    match thread_model.update(connection).await {
        Ok(_) => {
            tracing::info!("Successfully unlocked workflow thread {}", thread.id);
        }
        Err(e) => {
            tracing::error!(
                "Failed to unlock workflow thread {}: {}. This may cause the thread to remain locked.",
                thread.id,
                e
            );
        }
    }
}

async fn ensure_workflow_thread_unlocked(
    thread: &entity::threads::Model,
    connection: &sea_orm::DatabaseConnection,
) {
    if thread.is_processing {
        unlock_workflow_thread(thread, connection).await;
    }
}

/// Execute a workflow associated with a thread and stream results
///
/// Runs a workflow linked to a specific thread, streaming execution logs via SSE.
/// Locks the thread during execution to prevent concurrent modifications. On completion,
/// updates the thread with workflow results and unlocks it. Handles errors gracefully
/// by ensuring thread unlock even on failure.
#[utoipa::path(
    method(post),
    path = "/{project_id}/workflows/{pathb64}/run-thread",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("pathb64" = String, Path, description = "Thread ID or encoded id")
    ),
    request_body = RunWorkflowRequest,
    responses(
        (status = 200, description = "Workflow thread execution started successfully - returns streaming logs", content_type = "text/event-stream"),
        (status = 400, description = "Bad request - invalid thread ID"),
        (status = 404, description = "Thread not found"),
        (status = 409, description = "Thread is already being processed"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Automations"
)]
pub async fn run_workflow_thread(
    Path((_project_id, id)): Path<(Uuid, String)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
) -> Result<impl IntoResponse, StatusCode> {
    let config_manager = project_manager.config_manager.clone();

    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let thread_id = Uuid::parse_str(&id).map_err(|e| {
        tracing::warn!("Invalid thread ID format '{}': {}", id, e);
        StatusCode::BAD_REQUEST
    })?;

    let thread = Threads::find_by_id(thread_id)
        .one(&connection)
        .await
        .map_err(|e| {
            tracing::error!("Database error finding thread {}: {}", thread_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            tracing::warn!("Thread {} not found", thread_id);
            StatusCode::NOT_FOUND
        })?;

    if thread.is_processing {
        tracing::warn!("Thread {} is already being processed", thread_id);
        return Err(StatusCode::CONFLICT);
    }

    // Lock the thread with proper error handling
    let mut thread_model: entity::threads::ActiveModel = thread.clone().into();
    thread_model.is_processing = ActiveValue::Set(true);
    thread_model.update(&connection).await.map_err(|e| {
        tracing::error!("Failed to lock workflow thread {}: {}", thread_id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let workflow_ref = PathBuf::from(thread.source.to_string());
    let full_workflow_path = config_manager
        .resolve_file(workflow_ref.clone())
        .map_err(|e| {
            tracing::info!("{:?}", e);
            StatusCode::BAD_REQUEST
        })
        .await?;

    let full_workflow_path = PathBuf::from(&full_workflow_path);

    let streaming_workflow_persister = Arc::new(
        StreamingWorkflowPersister::new(connection.clone(), thread.clone())
            .await
            .map_err(|err| {
                tracing::error!(
                    "Failed to create streaming workflow handler for thread {}: {}",
                    thread_id,
                    err
                );
                let connection = connection.clone();
                let thread_clone = thread.clone();
                tokio::spawn(async move {
                    ensure_workflow_thread_unlocked(&thread_clone, &connection).await;
                });
                StatusCode::INTERNAL_SERVER_ERROR
            })?,
    );

    let (logger, receiver) =
        build_workflow_api_logger(&full_workflow_path, Some(streaming_workflow_persister)).await;

    let connection_clone = connection.clone();
    let thread_clone = thread.clone();
    let thread_user_id = thread.user_id;

    let _ = tokio::spawn(async move {
        let result = service::run_workflow(
            &workflow_ref,
            logger,
            RetryStrategy::NoRetry { variables: None },
            project_manager.clone(),
            None,
            None,
            None, // No globals for thread execution (not in request)
            thread_user_id,
        )
        .await;

        match result {
            Ok(_) => {
                if let Ok(logs) = service::get_workflow_logs(&workflow_ref, config_manager).await {
                    let mut thread_model: entity::threads::ActiveModel =
                        thread_clone.clone().into();
                    let logs_json = serde_json::to_string(&logs).unwrap_or_default();
                    thread_model.output = ActiveValue::Set(logs_json);
                    thread_model.is_processing = ActiveValue::Set(false);

                    if let Err(e) = thread_model.update(&connection_clone).await {
                        tracing::error!(
                            "Failed to update thread {} with workflow results: {}",
                            thread_clone.id,
                            e
                        );
                        unlock_workflow_thread(&thread_clone, &connection_clone).await;
                    } else {
                        tracing::info!("Thread {} updated with workflow logs", thread_clone.id);
                    }
                } else {
                    tracing::error!("Failed to get workflow logs for thread {}", thread_clone.id);
                    unlock_workflow_thread(&thread_clone, &connection_clone).await;
                }
            }
            Err(e) => {
                tracing::error!(
                    "Workflow execution failed for thread {}: {}",
                    thread_clone.id,
                    e
                );
                unlock_workflow_thread(&thread_clone, &connection_clone).await;
            }
        }
    });

    let stream = create_sse_stream(receiver);
    Ok(Sse::new(stream))
}

// Synchronous response for workflow thread execution
// Extends the sync API pattern to workflow threads, providing the same benefits:
// - Simple request-response pattern for thread-based workflows
// - Database thread locking handled automatically
// - Thread output included in response for completed executions
#[derive(Serialize, ToSchema)]
pub struct RunWorkflowThreadSyncResponse {
    pub logs: Vec<LogItem>,
    pub success: bool,
    pub completed: bool,
    pub error_message: Option<String>,
    pub thread_output: Option<String>,
    pub run_id: Option<i32>,
    pub events: Vec<WorkflowEvent>,
    #[schema(value_type = Object)]
    pub content: Option<serde_json::Value>,
}

#[derive(Deserialize, ToSchema)]
pub struct RunWorkflowThreadRequest {
    #[serde(default)]
    pub filters: Option<SessionFilters>,

    #[serde(default)]
    pub connections: Option<ConnectionOverrides>,
}

/// Execute a workflow thread and wait for completion (synchronous)
///
/// Runs a workflow associated with a thread and returns complete results after execution.
/// Locks the thread during execution. Supports configurable timeout via X-Oxy-Request-Timeout
/// header. Returns partial results if timeout is exceeded. Thread is unlocked on completion
/// or error.
#[utoipa::path(
    method(post),
    path = "/{project_id}/workflows/{pathb64}/run-thread-sync",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("pathb64" = String, Path, description = "Thread ID or encoded id")
    ),
    request_body = RunWorkflowThreadRequest,
    responses(
        (status = 200, description = "Workflow thread response (may be partial if timeout occurred)", body = RunWorkflowThreadSyncResponse),
        (status = 400, description = "Bad request - invalid thread ID or parameters"),
        (status = 404, description = "Thread not found"),
        (status = 409, description = "Thread is already being processed"),
        (status = 500, description = "Internal server error or workflow execution failed")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Automations"
)]
pub async fn run_workflow_thread_sync(
    Path((_project_id, id)): Path<(Uuid, String)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    timeout_config: TimeoutConfig,
    extract::Json(request): extract::Json<RunWorkflowThreadRequest>,
) -> Result<extract::Json<RunWorkflowThreadSyncResponse>, StatusCode> {
    let config_manager = project_manager.config_manager.clone();

    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let thread_id = Uuid::parse_str(&id).map_err(|e| {
        tracing::warn!("Invalid thread ID format '{}': {}", id, e);
        StatusCode::BAD_REQUEST
    })?;

    let thread = Threads::find_by_id(thread_id)
        .one(&connection)
        .await
        .map_err(|e| {
            tracing::error!("Database error finding thread {}: {}", thread_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            tracing::warn!("Thread {} not found", thread_id);
            StatusCode::NOT_FOUND
        })?;

    if thread.is_processing {
        tracing::warn!("Thread {} is already being processed", thread_id);
        return Err(StatusCode::CONFLICT);
    }

    let mut thread_model: entity::threads::ActiveModel = thread.clone().into();
    thread_model.is_processing = ActiveValue::Set(true);
    thread_model.update(&connection).await.map_err(|e| {
        tracing::error!("Failed to lock workflow thread {}: {}", thread_id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let workflow_ref = PathBuf::from(thread.source.to_string());
    let full_workflow_path = config_manager
        .resolve_file(workflow_ref.clone())
        .map_err(|e| {
            tracing::info!("Failed to resolve workflow path: {:?}", e);
            let connection = connection.clone();
            let thread_clone = thread.clone();
            tokio::spawn(async move {
                ensure_workflow_thread_unlocked(&thread_clone, &connection).await;
            });
            StatusCode::BAD_REQUEST
        })
        .await?;

    let full_workflow_path = PathBuf::from(&full_workflow_path);

    let streaming_workflow_persister = Arc::new(
        StreamingWorkflowPersister::new(connection.clone(), thread.clone())
            .await
            .map_err(|err| {
                tracing::error!(
                    "Failed to create streaming workflow handler for thread {}: {}",
                    thread_id,
                    err
                );
                let connection = connection.clone();
                let thread_clone = thread.clone();
                tokio::spawn(async move {
                    ensure_workflow_thread_unlocked(&thread_clone, &connection).await;
                });
                StatusCode::INTERNAL_SERVER_ERROR
            })?,
    );

    let (logger, mut receiver) =
        build_workflow_api_logger(&full_workflow_path, Some(streaming_workflow_persister)).await;

    let connection_clone = connection.clone();
    let thread_clone = thread.clone();
    let config_manager_clone = config_manager.clone();
    let workflow_ref_clone = workflow_ref.clone();
    let filters = request.filters;
    let connections = request.connections;
    let thread_user_id = thread.user_id;

    let mut workflow_task = tokio::spawn(async move {
        let result = service::run_workflow(
            &workflow_ref_clone,
            logger,
            RetryStrategy::NoRetry { variables: None },
            project_manager.clone(),
            filters,
            connections,
            None, // No globals for thread sync execution (not in request)
            thread_user_id,
        )
        .await;

        match result {
            Ok(_) => {
                if let Ok(logs) =
                    service::get_workflow_logs(&workflow_ref_clone, config_manager_clone).await
                {
                    let mut thread_model: entity::threads::ActiveModel =
                        thread_clone.clone().into();
                    let logs_json = serde_json::to_string(&logs).unwrap_or_default();
                    thread_model.output = ActiveValue::Set(logs_json.clone());
                    thread_model.is_processing = ActiveValue::Set(false);

                    if let Err(e) = thread_model.update(&connection_clone).await {
                        tracing::error!(
                            "Failed to update thread {} with workflow results: {}",
                            thread_clone.id,
                            e
                        );
                        unlock_workflow_thread(&thread_clone, &connection_clone).await;
                        Err(format!("Failed to update thread: {}", e))
                    } else {
                        tracing::info!("Thread {} updated with workflow logs", thread_clone.id);
                        Ok(logs_json)
                    }
                } else {
                    tracing::error!("Failed to get workflow logs for thread {}", thread_clone.id);
                    unlock_workflow_thread(&thread_clone, &connection_clone).await;
                    Err("Failed to get workflow logs".to_string())
                }
            }
            Err(e) => {
                tracing::error!(
                    "Workflow execution failed for thread {}: {}",
                    thread_clone.id,
                    e
                );
                unlock_workflow_thread(&thread_clone, &connection_clone).await;
                Err(format!("Workflow execution failed: {}", e))
            }
        }
    });

    let mut all_logs = Vec::new();

    let timeout_duration = timeout_config.duration;
    let start_time = std::time::Instant::now();

    let workflow_result = loop {
        if start_time.elapsed() >= timeout_duration {
            tracing::info!(
                "Workflow thread {} timed out after {:?}, returning partial results",
                thread.id,
                timeout_duration
            );
            let connection = connection.clone();
            let thread_clone = thread.clone();
            tokio::spawn(async move {
                ensure_workflow_thread_unlocked(&thread_clone, &connection).await;
            });
            break Err("Timeout reached, workflow still running".to_string());
        }

        tokio::select! {
            log_result = receiver.recv() => {
                match log_result {
                    Some(log_item) => {
                        all_logs.push(log_item);
                    }
                    None => {
                        break match workflow_task.await {
                            Ok(result) => result,
                            Err(e) => {
                                tracing::error!("Failed to join workflow task: {:?}", e);
                                let connection = connection.clone();
                                let thread_clone = thread.clone();
                                tokio::spawn(async move {
                                    ensure_workflow_thread_unlocked(&thread_clone, &connection).await;
                                });
                                Err(format!("Failed to join workflow task: {:?}", e))
                            }
                        };
                    }
                }
            }
            task_result = &mut workflow_task => {
                break match task_result {
                    Ok(result) => result,
                    Err(e) => {
                        tracing::error!("Failed to join workflow task: {:?}", e);
                        let connection = connection.clone();
                        let thread_clone = thread.clone();
                        tokio::spawn(async move {
                            ensure_workflow_thread_unlocked(&thread_clone, &connection).await;
                        });
                        Err(format!("Failed to join workflow task: {:?}", e))
                    }
                };
            }
        }
    };

    while let Ok(log_item) = receiver.try_recv() {
        all_logs.push(log_item);
    }

    match workflow_result {
        Ok(thread_output) => {
            tracing::info!(
                "Workflow thread completed successfully with {} logs",
                all_logs.len()
            );

            Ok(extract::Json(RunWorkflowThreadSyncResponse {
                logs: all_logs,
                success: true,
                completed: true,
                error_message: None,
                thread_output: Some(thread_output),
                run_id: Some(0), // Threads don't use run tracking
                events: vec![],
                content: None,
            }))
        }
        Err(error_msg) => {
            let is_timeout = error_msg.contains("Timeout reached");
            if is_timeout {
                tracing::info!(
                    "Workflow thread {} timed out with {} logs collected so far",
                    thread.id,
                    all_logs.len()
                );

                Ok(extract::Json(RunWorkflowThreadSyncResponse {
                    logs: all_logs,
                    success: false,
                    completed: false, // Not completed due to timeout
                    error_message: Some(format!("Partial results: {}", error_msg)),
                    thread_output: None,
                    run_id: Some(0), // Threads don't use run tracking
                    events: vec![],
                    content: None,
                }))
            } else {
                tracing::error!(
                    "Workflow thread failed with {} logs: {}",
                    all_logs.len(),
                    error_msg
                );

                Ok(extract::Json(RunWorkflowThreadSyncResponse {
                    logs: all_logs,
                    success: false,
                    completed: true, // Completed with error
                    error_message: Some(error_msg),
                    thread_output: None,
                    run_id: Some(0), // Threads don't use run tracking
                    events: vec![],
                    content: None,
                }))
            }
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct RunWorkflowSyncResponse {
    pub logs: Vec<LogItem>,
    pub success: bool,
    pub completed: bool,
    pub error_message: Option<String>,
    pub run_id: Option<i32>,
    pub events: Vec<WorkflowEvent>,
    #[schema(value_type = Object)]
    pub content: Option<serde_json::Value>,
}

/// Execute a workflow and wait for completion (synchronous)
///
/// Runs a workflow and returns complete results after execution finishes. Supports
/// configurable timeout via X-Oxy-Request-Timeout header (default: 60s). Returns
/// partial results with completed=false if timeout is exceeded. Tracks execution
/// in workflow tracker for status monitoring.
#[utoipa::path(
    method(post),
    path = "/{project_id}/workflows/{pathb64}/run-sync",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("pathb64" = String, Path, description = "Base64 encoded path to the workflow")
    ),
    request_body = RunWorkflowRequest,
    responses(
        (status = 200, description = "Workflow response (may be partial if timeout occurred). Use X-Oxy-Request-Timeout header to specify timeout in seconds (default: 60, configurable via OXY_REQUEST_TIMEOUT_SECS)", body = RunWorkflowSyncResponse),
        (status = 400, description = "Bad request - invalid path or parameters"),
        (status = 408, description = "Request timeout - use global timeout layer"),
        (status = 500, description = "Internal server error or workflow execution failed")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Automations"
)]
pub async fn run_workflow_sync(
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    timeout_config: TimeoutConfig,
    extract::Json(request): extract::Json<RunWorkflowRequest>,
) -> Result<extract::Json<RunWorkflowSyncResponse>, StatusCode> {
    let decoded_path = BASE64_STANDARD.decode(&pathb64).map_err(|e| {
        tracing::info!("Failed to decode base64 path: {:?}", e);
        StatusCode::BAD_REQUEST
    })?;

    let source_id = String::from_utf8(decoded_path).map_err(|e| {
        tracing::info!("Failed to convert decoded path to string: {:?}", e);
        StatusCode::BAD_REQUEST
    })?;

    let path = PathBuf::from(&source_id);

    let full_workflow_path = project_manager
        .config_manager
        .resolve_file(path.clone())
        .map_err(|e| {
            tracing::info!("Failed to resolve workflow path: {:?}", e);
            StatusCode::BAD_REQUEST
        })
        .await?;

    let full_workflow_path = PathBuf::from(&full_workflow_path);

    let timeout_duration = timeout_config.duration;

    let retry_strategy = if let Some(retry_param) = &request.retry_param {
        RetryStrategy::Retry {
            replay_id: retry_param.replay_id.clone(),
            run_index: retry_param.run_id.parse().unwrap_or(0),
        }
    } else {
        RetryStrategy::NoRetry {
            variables: request.variables.clone(),
        }
    };

    // Generate a run_id and create broadcast topic
    let runs_manager = project_manager.runs_manager.as_ref().ok_or_else(|| {
        tracing::error!("RunsManager not initialized");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let run_info = runs_manager
        .new_run(&source_id, request.variables.clone(), None, Some(user.id))
        .await
        .map_err(|e| {
            tracing::error!("Failed to create new run: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let run_id = run_info.run_index.ok_or_else(|| {
        tracing::error!("Run index not available");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let task_id = format!("{}::{}", source_id, run_id);

    tracing::info!("Starting workflow run with task_id: {}", task_id);

    // Create broadcast topic for this run
    let topic_ref = BROADCASTER.create_topic(&task_id).await.map_err(|e| {
        tracing::error!("Failed to create broadcast topic: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Also create logger for backward compatibility
    let (_logger, mut log_receiver) = build_workflow_api_logger(&full_workflow_path, None).await;

    // Subscribe to broadcast to collect events
    let subscription = BROADCASTER.subscribe(&task_id).await.map_err(|e| {
        tracing::error!("Failed to subscribe to broadcast topic: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut event_receiver = subscription.receiver;
    let mut all_events = subscription.items;

    let filters = request.filters;
    let connections = request.connections;
    let globals = request.globals;

    let mut workflow_task = tokio::spawn({
        let project_manager = project_manager.clone();
        let path = path.clone();
        let task_id_clone = task_id.clone();
        async move {
            // Use run_workflow_v2 with TopicRef as event handler
            let result = service::run_workflow_v2(
                project_manager.clone(),
                path.clone(),
                topic_ref,
                retry_strategy,
                filters,
                connections,
                globals,
                Some(user.id),
            )
            .await;

            match result {
                Ok(_) => {
                    tracing::info!("Workflow task_id={} completed successfully", task_id_clone);
                    Ok(())
                }
                Err(e) => {
                    tracing::error!(
                        "Workflow task_id={} execution failed: {:?}",
                        task_id_clone,
                        e
                    );
                    Err(e)
                }
            }
        }
    });

    let mut all_logs = Vec::new();
    let start_time = std::time::Instant::now();

    let workflow_result = loop {
        if start_time.elapsed() >= timeout_duration {
            tracing::info!(
                "Workflow task_id={} timed out after {:?}, returning partial results",
                task_id,
                timeout_duration
            );
            break Err("Timeout reached, workflow still running".to_string());
        }

        tokio::select! {
            log_result = log_receiver.recv() => {
                match log_result {
                    Some(log_item) => {
                        all_logs.push(log_item);
                    }
                    None => {
                        // Log channel closed, keep collecting events until workflow completes
                    }
                }
            }
            event_result = event_receiver.recv() => {
                match event_result {
                    Ok(event) => {
                        // Check if this is a workflow finished event
                        let is_finished = matches!(
                            &event,
                            crate::service::types::event::EventKind::WorkflowFinished { .. }
                        );
                        all_events.push(event);
                        if is_finished {
                            // Wait for workflow task to complete
                            break match workflow_task.await {
                                Ok(result) => result.map_err(|e| {
                                    tracing::error!("Workflow execution error: {:?}", e);
                                    format!("Workflow execution error: {:?}", e)
                                }),
                                Err(e) => {
                                    tracing::error!("Failed to join workflow task: {:?}", e);
                                    Err(format!("Failed to join workflow task: {:?}", e))
                                }
                            };
                        }
                    }
                    Err(_) => {
                        // Channel closed, workflow must have finished
                        break match workflow_task.await {
                            Ok(result) => result.map_err(|e| {
                                tracing::error!("Workflow execution error: {:?}", e);
                                format!("Workflow execution error: {:?}", e)
                            }),
                            Err(e) => {
                                tracing::error!("Failed to join workflow task: {:?}", e);
                                Err(format!("Failed to join workflow task: {:?}", e))
                            }
                        };
                    }
                }
            }
            task_result = &mut workflow_task => {
                break match task_result {
                    Ok(result) => result.map_err(|e| {
                        tracing::error!("Workflow execution error: {:?}", e);
                        format!("Workflow execution error: {:?}", e)
                    }),
                    Err(e) => {
                        tracing::error!("Failed to join workflow task: {:?}", e);
                        Err(format!("Failed to join workflow task: {:?}", e))
                    }
                };
            }
        }
    };

    // Collect remaining logs and events
    while let Ok(log_item) = log_receiver.try_recv() {
        all_logs.push(log_item);
    }

    while let Ok(event) = event_receiver.try_recv() {
        all_events.push(event);
    }

    // Parse events to consolidate content
    let (_status, _error, _success, _completed, content) = parse_workflow_events(&all_events);

    // Convert events to API format, filtering out intermediate ContentAdded
    let api_events: Vec<WorkflowEvent> = all_events
        .iter()
        .filter_map(|event| {
            let api_event = WorkflowEvent::from(event);
            // Filter out intermediate ContentAdded events
            match &api_event {
                WorkflowEvent::ContentAdded { .. } => None,
                _ => Some(api_event),
            }
        })
        .collect();

    match workflow_result {
        Ok(_) => {
            tracing::info!(
                "Workflow task_id={} completed successfully with {} logs, {} events",
                task_id,
                all_logs.len(),
                all_events.len()
            );

            Ok(extract::Json(RunWorkflowSyncResponse {
                logs: all_logs,
                success: true,
                completed: true,
                error_message: None,
                run_id: Some(run_id),
                events: api_events,
                content,
            }))
        }
        Err(error_msg) => {
            let is_timeout = error_msg.contains("Timeout reached");
            if is_timeout {
                tracing::info!(
                    "Workflow task_id={} timed out with {} logs, {} events collected so far",
                    task_id,
                    all_logs.len(),
                    all_events.len()
                );

                Ok(extract::Json(RunWorkflowSyncResponse {
                    logs: all_logs,
                    success: false,
                    completed: false, // Not completed due to timeout
                    error_message: Some(format!("Partial results: {}", error_msg)),
                    run_id: Some(run_id),
                    events: api_events,
                    content,
                }))
            } else {
                tracing::error!(
                    "Workflow task_id={} failed with {} logs, {} events: {}",
                    task_id,
                    all_logs.len(),
                    all_events.len(),
                    error_msg
                );

                Ok(extract::Json(RunWorkflowSyncResponse {
                    logs: all_logs,
                    success: false,
                    completed: true, // Completed with error
                    error_message: Some(error_msg),
                    run_id: Some(run_id),
                    events: api_events,
                    content,
                }))
            }
        }
    }
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct CreateFromQueryRequest {
    pub query: String,
    pub prompt: String,
    pub database: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct CreateFromQueryResponse {
    #[schema(value_type = Object)]
    pub workflow: Workflow,
}

/// Generate a workflow from a natural language query
///
/// Creates a workflow configuration automatically from a natural language query and
/// database schema. Uses AI to interpret the query and generate appropriate workflow
/// steps, transformations, and outputs based on the specified database context.
#[utoipa::path(
    method(post),
    path = "/{project_id}/workflows/from-query",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID")
    ),
    request_body = CreateFromQueryRequest,
    responses(
        (status = 200, description = "Workflow created successfully from query", body = CreateFromQueryResponse),
        (status = 400, description = "Bad request - invalid query parameters"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Automations"
)]
pub async fn create_from_query(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    extract::Json(request): extract::Json<CreateFromQueryRequest>,
) -> Result<extract::Json<CreateFromQueryResponse>, StatusCode> {
    let config_manager = project_manager.config_manager;
    let workflow = service::create_workflow_from_query(
        &request.query,
        &request.prompt,
        &request.database,
        &config_manager,
    )
    .await
    .map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    Ok(extract::Json(CreateFromQueryResponse { workflow }))
}

// =================================================================================================
// UNIFIED WORKFLOW RUN STATUS API
// =================================================================================================

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum WorkflowEvent {
    WorkflowStarted {
        workflow_id: String,
        run_id: String,
    },
    WorkflowFinished {
        workflow_id: String,
        run_id: String,
        error: Option<String>,
    },
    TaskStarted {
        task_id: String,
        task_name: String,
    },
    TaskFinished {
        task_id: String,
        error: Option<String>,
    },
    ArtifactStarted {
        artifact_id: String,
        artifact_name: String,
    },
    ArtifactFinished {
        artifact_id: String,
        error: Option<String>,
    },
    ContentAdded {
        content_id: String,
        content: serde_json::Value,
    },
    ContentDone {
        content_id: String,
        content: serde_json::Value,
    },
}

impl From<&crate::service::types::event::EventKind> for WorkflowEvent {
    fn from(event: &crate::service::types::event::EventKind) -> Self {
        use crate::service::types::event::EventKind;

        match event {
            EventKind::WorkflowStarted {
                workflow_id,
                run_id,
                ..
            } => WorkflowEvent::WorkflowStarted {
                workflow_id: workflow_id.clone(),
                run_id: run_id.clone(),
            },
            EventKind::WorkflowFinished {
                workflow_id,
                run_id,
                error,
            } => WorkflowEvent::WorkflowFinished {
                workflow_id: workflow_id.clone(),
                run_id: run_id.clone(),
                error: error.clone(),
            },
            EventKind::TaskStarted {
                task_id, task_name, ..
            } => WorkflowEvent::TaskStarted {
                task_id: task_id.clone(),
                task_name: task_name.clone(),
            },
            EventKind::TaskFinished { task_id, error } => WorkflowEvent::TaskFinished {
                task_id: task_id.clone(),
                error: error.clone(),
            },
            EventKind::ArtifactStarted {
                artifact_id,
                artifact_name,
                ..
            } => WorkflowEvent::ArtifactStarted {
                artifact_id: artifact_id.clone(),
                artifact_name: artifact_name.clone(),
            },
            EventKind::ArtifactFinished { artifact_id, error } => WorkflowEvent::ArtifactFinished {
                artifact_id: artifact_id.clone(),
                error: error.clone(),
            },
            EventKind::ContentAdded { content_id, item } => WorkflowEvent::ContentAdded {
                content_id: content_id.clone(),
                content: serde_json::to_value(item).unwrap_or(serde_json::Value::Null),
            },
            EventKind::ContentDone { content_id, item } => WorkflowEvent::ContentDone {
                content_id: content_id.clone(),
                content: serde_json::to_value(item).unwrap_or(serde_json::Value::Null),
            },
            // Skip TaskMetadata events as they're internal
            EventKind::TaskMetadata { .. } => {
                // Return a placeholder - we'll filter these out later
                WorkflowEvent::TaskStarted {
                    task_id: String::new(),
                    task_name: String::from("internal_metadata"),
                }
            }
            _ => {
                // For any other event types, return a generic placeholder
                WorkflowEvent::TaskStarted {
                    task_id: String::new(),
                    task_name: String::from("internal_metadata"),
                }
            }
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct WorkflowRunResponse {
    pub run_id: i32,
    pub status: String,
    pub success: Option<bool>,
    pub completed: bool,
    pub error_message: Option<String>,
    pub events: Vec<WorkflowEvent>,
    pub content: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
}

/// Get workflow run status with optional wait for completion
///
/// Retrieves the current status and data of a workflow run. When wait_for_completion=true,
/// the endpoint will block until the workflow completes or fails. Uses the broadcast system
/// to subscribe to workflow events and return the final result.
#[utoipa::path(
    method(get),
    path = "/{project_id}/workflows/{pathb64}/runs/{run_id}",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("pathb64" = String, Path, description = "Base64 encoded path to the workflow"),
        ("run_id" = String, Path, description = "Run identifier"),
        ("wait_for_completion" = Option<bool>, Query, description = "Wait for workflow to complete")
    ),
    responses(
        (status = 200, description = "Run status and events retrieved successfully", body = WorkflowRunResponse),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Runs"
)]
pub async fn get_workflow_run(
    Path((_project_id, pathb64, run_id)): Path<(Uuid, String, i32)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    extract::Query(params): extract::Query<std::collections::HashMap<String, String>>,
) -> Result<extract::Json<WorkflowRunResponse>, StatusCode> {
    let wait_for_completion = params
        .get("wait_for_completion")
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false);

    tracing::info!(
        "Getting status for run: {} in workflow: {} (wait: {})",
        run_id,
        pathb64,
        wait_for_completion
    );

    let decoded_path = BASE64_STANDARD.decode(&pathb64).map_err(|e| {
        tracing::error!("Failed to decode base64 path: {:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let source_id = String::from_utf8(decoded_path).map_err(|e| {
        tracing::error!("Failed to convert decoded path to string: {:?}", e);
        StatusCode::BAD_REQUEST
    })?;

    let task_id = format!("{}::{}", source_id, run_id);

    // Try to subscribe to the broadcast topic (for active runs)
    let subscription_result = BROADCASTER.subscribe(&task_id).await;

    let (events, from_broadcast) = match subscription_result {
        Ok(subscription) => {
            // Active run found in broadcast system
            let mut events = subscription.items;
            let mut receiver = subscription.receiver;

            // If wait_for_completion is true, listen for completion event
            if wait_for_completion {
                loop {
                    match receiver.recv().await {
                        Ok(event) => {
                            events.push(event.clone());
                            // Check if this is a workflow finished event
                            if matches!(
                                &event,
                                crate::service::types::event::EventKind::WorkflowFinished { .. }
                            ) {
                                break;
                            }
                        }
                        Err(_) => {
                            // Channel closed, workflow must have finished
                            break;
                        }
                    }
                }
            }
            (events, true)
        }
        Err(_) => {
            // Not found in broadcast, try database for historical runs
            tracing::info!("Run {} not found in broadcast, querying database", task_id);

            let runs_manager = project_manager.runs_manager.as_ref().ok_or_else(|| {
                tracing::error!("RunsManager not available");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

            let run_details = runs_manager
                .find_run_details(&source_id, Some(run_id))
                .await
                .map_err(|e| {
                    tracing::error!("Failed to query run from database: {:?}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?
                .ok_or_else(|| {
                    tracing::warn!("Run not found: {}", task_id);
                    StatusCode::NOT_FOUND
                })?;

            // Convert RunDetails to events for consistent response format
            let events = reconstruct_events_from_run_details(&run_details);
            (events, false)
        }
    };

    // Parse events to determine status and extract data
    let (_status, error_message, success, completed, content) = parse_workflow_events(&events);

    // Convert EventKind to WorkflowEvent for API response
    // When wait_for_completion=true or from database, filter out intermediate ContentAdded events
    // since they're consolidated in the content field
    let api_events: Vec<WorkflowEvent> = events
        .iter()
        .filter(|e| {
            if wait_for_completion || !from_broadcast {
                // Skip intermediate ContentAdded events - they're merged in content field
                !matches!(e, crate::service::types::event::EventKind::ContentAdded { .. })
            } else {
                true
            }
        })
        .map(WorkflowEvent::from)
        .filter(|e| {
            // Filter out internal metadata events
            !matches!(e, WorkflowEvent::TaskStarted { task_name, .. } if task_name == "internal_metadata")
        })
        .collect();

    let runs_manager = project_manager.runs_manager.ok_or_else(|| {
        tracing::error!("RunsManager not available");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let run_details = runs_manager
        .find_run_details(&source_id, Some(run_id))
        .await
        .map_err(|e| {
            tracing::error!("Failed to query run from database: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            tracing::warn!("Run not found: {}", task_id);
            StatusCode::NOT_FOUND
        })?;
    let topics = BROADCASTER.list_topics::<HashSet<String>>().await;
    let mut run_info = run_details.run_info;
    let task_id = match &run_info.root_ref {
        Some(root_ref) => root_ref.task_id().ok(),
        None => run_info.task_id().ok(),
    };
    if let Some(task_id) = task_id {
        let status = match (topics.contains(&task_id), &run_info.status) {
            (true, _) => RunStatus::Running,
            (false, RunStatus::Pending) => RunStatus::Canceled,
            _ => run_info.status.clone(),
        };
        run_info.set_status(status);
    }
    Ok(extract::Json(WorkflowRunResponse {
        run_id,
        status: run_info.status.to_string(),
        success,
        completed,
        error_message,
        events: api_events,
        content,
        output: run_details.output,
    }))
}

fn parse_workflow_events(
    events: &[crate::service::types::event::EventKind],
) -> (
    String,
    Option<String>,
    Option<bool>,
    bool,
    Option<serde_json::Value>,
) {
    use crate::service::types::content::ContentType;
    use crate::service::types::event::EventKind;
    use std::collections::HashMap;

    let mut status = "running".to_string();
    let mut error_message = None;
    let mut success = None;
    let mut completed = false;

    let mut content_map: HashMap<String, (ContentType, bool)> = HashMap::new();

    for event in events {
        match event {
            EventKind::WorkflowStarted { .. } => {
                status = "running".to_string();
            }
            EventKind::WorkflowFinished { error, .. } => {
                if let Some(err) = error {
                    status = "failed".to_string();
                    error_message = Some(err.clone());
                    success = Some(false);
                } else {
                    status = "completed".to_string();
                    success = Some(true);
                }
                completed = true;
            }
            EventKind::ContentAdded { content_id, item } => {
                content_map
                    .entry(content_id.clone())
                    .and_modify(|(existing_content, _)| {
                        // Merge text content chunks
                        if let (
                            ContentType::Text { content: existing },
                            ContentType::Text { content: new },
                        ) = (existing_content, item)
                        {
                            existing.push_str(new);
                        }
                    })
                    .or_insert((item.clone(), false));
            }
            EventKind::ContentDone { content_id, item } => {
                content_map
                    .entry(content_id.clone())
                    .and_modify(|(existing_content, is_done)| {
                        // Merge any final chunk
                        if let (
                            ContentType::Text { content: existing },
                            ContentType::Text { content: new },
                        ) = (existing_content, item)
                        {
                            existing.push_str(new);
                        }
                        *is_done = true;
                    })
                    .or_insert((item.clone(), true));
            }
            _ => {}
        }
    }

    let content_items: Vec<serde_json::Value> = content_map
        .into_iter()
        .filter_map(|(content_id, (content_type, is_done))| {
            // Only include content that's marked as done
            if is_done {
                let mut json = serde_json::to_value(&content_type).ok()?;
                // Add content_id to the output for reference
                if let Some(obj) = json.as_object_mut() {
                    obj.insert(
                        "content_id".to_string(),
                        serde_json::Value::String(content_id),
                    );
                }
                Some(json)
            } else {
                None
            }
        })
        .collect();

    let content = if content_items.is_empty() {
        None
    } else {
        Some(serde_json::json!({ "items": content_items }))
    };

    (status, error_message, success, completed, content)
}

/// Reconstruct events from database RunDetails for historical runs
fn reconstruct_events_from_run_details(
    run_details: &crate::service::types::run::RunDetails,
) -> Vec<crate::service::types::event::EventKind> {
    use crate::service::types::block::BlockKind;
    use crate::service::types::content::ContentType;
    use crate::service::types::event::EventKind;

    let mut events = Vec::new();
    let run_info = &run_details.run_info;

    // Add WorkflowStarted event
    events.push(EventKind::WorkflowStarted {
        workflow_id: run_info.source_id.clone(),
        run_id: run_info.run_index.unwrap_or(0).to_string(),
        workflow_config: crate::config::model::Workflow {
            name: run_info.source_id.clone(),
            tasks: Vec::new(),
            tests: Vec::new(),
            variables: None,
            description: String::new(),
            retrieval: None,
            consistency_prompt: None,
        },
    });

    // Process blocks if available
    if let Some(blocks) = &run_details.blocks {
        for (block_id, block) in blocks.iter() {
            match &block.block_kind {
                BlockKind::Task {
                    task_name,
                    task_metadata,
                } => {
                    events.push(EventKind::TaskStarted {
                        task_id: block_id.clone(),
                        task_name: task_name.clone(),
                        task_metadata: task_metadata.clone(),
                    });

                    // Add task finished event
                    events.push(EventKind::TaskFinished {
                        task_id: block_id.clone(),
                        error: block.error.clone(),
                    });
                }
                BlockKind::Text { content } => {
                    // Convert text block to ContentDone event
                    events.push(EventKind::ContentDone {
                        content_id: block_id.clone(),
                        item: ContentType::Text {
                            content: content.clone(),
                        },
                    });
                }
                BlockKind::SQL {
                    sql_query,
                    database,
                    result,
                    is_result_truncated,
                } => {
                    // Convert SQL block to ContentDone event
                    events.push(EventKind::ContentDone {
                        content_id: block_id.clone(),
                        item: ContentType::SQL {
                            sql_query: sql_query.clone(),
                            database: database.clone(),
                            result: result.clone(),
                            is_result_truncated: *is_result_truncated,
                        },
                    });
                }
                BlockKind::Group { .. } => {
                    // Skip group blocks, they're just containers
                }
                _ => {
                    // Handle other block kinds as needed
                }
            }
        }
    }

    // Add WorkflowFinished event
    let (success, error) = match &run_info.status {
        crate::service::types::run::RunStatus::Completed => (true, None),
        crate::service::types::run::RunStatus::Failed => (false, run_details.error.clone()),
        crate::service::types::run::RunStatus::Canceled => {
            (false, Some("Workflow was canceled".to_string()))
        }
        _ => (false, Some("Workflow did not complete".to_string())),
    };

    events.push(EventKind::WorkflowFinished {
        workflow_id: run_info.source_id.clone(),
        run_id: run_info.run_index.unwrap_or(0).to_string(),
        error: if success { None } else { error },
    });

    events
}
