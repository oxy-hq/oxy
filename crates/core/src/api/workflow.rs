use base64::prelude::*;
use entity::prelude::Threads;
use futures::TryFutureExt;
use sea_orm::ActiveValue;
use sea_orm::EntityTrait;
use serde::Deserialize;
use std::path::PathBuf;
use utoipa::ToSchema;
use uuid::Uuid;

use std::fs::File;
use std::sync::Arc;
use std::sync::Mutex;

use crate::api::middlewares::project::ProjectManagerExtractor;
use crate::config::model::Workflow;
use crate::service::thread::streaming_workflow_persister::StreamingWorkflowPersister;
use crate::service::workflow as service;
use crate::service::workflow::WorkflowInfo;
use crate::service::workflow::get_workflow;
use crate::service::workflow::run_workflow as run_workflow_service;
use crate::utils::create_sse_stream;
use crate::workflow::RetryStrategy;
use crate::workflow::loggers::api::WorkflowAPILogger;
use crate::workflow::loggers::types::LogItem;
use crate::workflow::loggers::types::WorkflowLogger;
use axum::extract::{self, Path};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::Sse;
use sea_orm::ActiveModelTrait;
use serde::Serialize;
use std::fs::OpenOptions;
use tokio::sync::mpsc;

use crate::db::client::establish_connection;

#[derive(Serialize)]
pub struct GetWorkflowResponse {
    data: Workflow,
}

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
    )
)]
pub async fn list(
    Path(_project_id): Path<Uuid>,
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

pub async fn get(
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
) -> Result<extract::Json<GetWorkflowResponse>, StatusCode> {
    let decoded_path = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let path = String::from_utf8(decoded_path).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;

    let config_manager = project_manager.config_manager;

    match get_workflow(PathBuf::from(path), config_manager.clone()).await {
        Ok(workflow) => Ok(extract::Json(GetWorkflowResponse { data: workflow })),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

#[derive(Serialize, ToSchema)]
pub struct GetLogsResponse {
    logs: Vec<LogItem>,
}

#[utoipa::path(
    method(get),
    path = "/{project_id}/workflows/{pathb64}/logs",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("pathb64" = String, Path, description = "Base64 encoded path to the workflow")
    ),
    responses(
        (status = 200, description = "Success", body = GetLogsResponse, content_type = "application/json")
    ),
    security(
        ("ApiKey" = [])
    )
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
    let full_workflow_path_b64: String =
        BASE64_STANDARD.encode(full_workflow_path.to_str().unwrap());
    // Create a channel to send logs to the client
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
    // None if not replaying, Some if replaying
    pub replay_id: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct RunWorkflowRequest {
    // variables: Option<HashMap<String, serde_json::Value>>,
    retry_param: Option<WorkflowRetryParam>,
}

#[utoipa::path(
    method(post),
    path = "/{project_id}/workflows/{pathb64}/run",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("pathb64" = String, Path, description = "Base64 encoded path to the workflow")
    ),
    responses(
        (status = 200, description = "Success", body = (), content_type = "text/event-stream")
    ),
    security(
        ("ApiKey" = [])
    )
)]
pub async fn run_workflow(
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
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

    let _ = tokio::spawn(async move {
        tracing::info!("Workflow run started");
        let rs = run_workflow_service(
            path,
            logger.clone(),
            RetryStrategy::NoRetry,
            None,
            project_manager.clone(),
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

#[utoipa::path(
    method(post),
    path = "/{project_id}/workflows/{pathb64}/run-thread",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("pathb64" = String, Path, description = "Thread ID or encoded id")
    ),
    responses(
        (status = 200, description = "Success", body = (), content_type = "text/event-stream")
    ),
    security(
        ("ApiKey" = [])
    )
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
                // Ensure thread is unlocked on error
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

    let _ = tokio::spawn(async move {
        let result = service::run_workflow(
            &workflow_ref,
            logger,
            RetryStrategy::NoRetry,
            None,
            project_manager.clone(),
        )
        .await;

        // Handle workflow completion or error
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
                        // Still try to unlock the thread
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

#[derive(Serialize, Deserialize)]
pub struct CreateFromQueryRequest {
    pub query: String,
    pub prompt: String,
    pub database: String,
}

#[derive(Serialize, Deserialize)]
pub struct CreateFromQueryResponse {
    pub workflow: Workflow,
}

pub async fn create_from_query(
    extract::Path(_project_id): Path<Uuid>,
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
