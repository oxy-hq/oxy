use std::collections::HashSet;
use std::path::PathBuf;

use axum::extract::{self, Path, Query};
use axum::http::StatusCode;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::adapters::runs::RunsManager;
use crate::errors::OxyError;
use crate::execute::writer::Handler;
use crate::service::block::GroupBlockHandler;
use crate::service::statics::BROADCASTER;
use crate::service::task_manager::TASK_MANAGER;
use crate::service::types::pagination::{Paginated, Pagination};
use crate::service::types::run::{RunDetails, RunInfo, RunStatus};
use crate::service::workflow::run_workflow_v2;
use crate::utils::{create_sse_broadcast, file_path_to_source_id};
use crate::workflow::RetryStrategy;

#[derive(serde::Deserialize, ToSchema)]
pub struct PaginationQuery {
    pub page: Option<usize>,
    pub size: Option<usize>,
}

#[utoipa::path(
    get,
    path = "workflows/{pathb64}/runs",
    params(
        ("page" = Option<usize>, Query, description = "Page number (default: 1)"),
        ("size" = Option<usize>, Query, description = "Items per page (default: 100, max: 100)")
    ),
    responses(
        (status = 200, description = "List of workflow runs with pagination", body = Paginated<RunInfo>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Runs"
)]
pub async fn get_workflow_runs(
    Path(pathb64): Path<String>,
    Query(pagination): Query<PaginationQuery>,
) -> Result<extract::Json<Paginated<RunInfo>>, StatusCode> {
    let decoded_path = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let path: PathBuf = PathBuf::from(String::from_utf8(decoded_path).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?);

    let mut result = RunsManager::default()
        .await?
        .list_runs(
            &file_path_to_source_id(&path),
            &Pagination {
                page: pagination.page.unwrap_or(1),
                size: pagination.size.unwrap_or(100),
                num_pages: None,
            },
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to list runs: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let topics = BROADCASTER.list_topics::<HashSet<String>>().await;
    for item in result.items.iter_mut() {
        let task_id = match &item.root_ref {
            Some(root_ref) => root_ref.task_id().ok(),
            None => item.task_id().ok(),
        };
        if let Some(task_id) = task_id {
            let status = match (topics.contains(&task_id), &item.status) {
                (true, _) => RunStatus::Running,
                (false, RunStatus::Pending) => RunStatus::Canceled,
                _ => item.status.clone(),
            };
            item.set_status(status);
        }
    }

    Ok(extract::Json(result))
}

#[derive(Deserialize, ToSchema)]
pub struct RetryParam {
    pub run_id: i32, // Run ID to retry
    // None if not replaying, Some if replaying
    pub replay_id: Option<String>,
}

#[derive(serde::Deserialize, ToSchema)]
pub struct CreateRunRequest {
    // variables: Option<HashMap<String, serde_json::Value>>,
    retry_param: Option<RetryParam>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, ToSchema)]
pub struct CreateRunResponse {
    pub run: RunInfo,
}

#[utoipa::path(
    post,
    path = "workflows/{pathb64}/runs",
    request_body = CreateRunRequest,
    responses(
        (status = 201, description = "Successfully create the workflow run", body = CreateRunResponse),
        (status = 404, description = "Workflow not found"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Runs"
)]
pub async fn create_workflow_run(
    Path(pathb64): Path<String>,
    extract::Json(payload): extract::Json<CreateRunRequest>,
) -> Result<extract::Json<CreateRunResponse>, StatusCode> {
    let decoded_path = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let path = PathBuf::from(String::from_utf8(decoded_path).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?);
    let runs_manager = RunsManager::default().await.map_err(|e| {
        tracing::error!("Failed to initialize RunsManager: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let run_info = match &payload.retry_param {
        None => runs_manager
            .new_run(&file_path_to_source_id(&path))
            .await
            .map_err(|e| {
                tracing::error!("Failed to create run: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            }),
        Some(retry_param) => {
            let run_id = retry_param.run_id;
            runs_manager
                .find_run(&file_path_to_source_id(&path), Some(run_id))
                .await
                .map_err(|e| {
                    tracing::error!("Failed to retry run: {:?}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?
                .ok_or_else(|| {
                    tracing::error!("Run with ID {} not found", retry_param.run_id);
                    StatusCode::NOT_FOUND
                })
        }
    }?;
    let replay_id = payload
        .retry_param
        .as_ref()
        .and_then(|p| p.replay_id.clone());
    let (run_info, replay_id) = match run_info.root_ref {
        Some(root_ref) => runs_manager
            .find_run(&root_ref.source_id, root_ref.run_index)
            .await
            .map_err(|e| {
                tracing::error!("Failed to find root run: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or(StatusCode::NOT_FOUND)
            .map(|run| {
                (
                    run,
                    replay_id.map(|id| {
                        if id.is_empty() {
                            root_ref.replay_ref.clone()
                        } else {
                            format!("{}.{}", root_ref.replay_ref, id)
                        }
                    }),
                )
            })?,
        None => (run_info, replay_id),
    };
    tracing::info!("Creating new run {:?} with {:?}", run_info, replay_id);

    let task_id = run_info.task_id()?;
    let topic_ref = BROADCASTER.create_topic(&task_id).await.map_err(|err| {
        tracing::error!("Failed to create topic for task ID {task_id}: {err}");
        StatusCode::BAD_REQUEST
    })?;
    let run_index = run_info.run_index.ok_or(StatusCode::BAD_REQUEST)?;
    let topic_id = task_id.clone();
    let callback_fn = async move || -> Result<(), OxyError> {
        // Handle the completion of the run and broadcast events
        if let Some(closed) = BROADCASTER.remove_topic(&topic_id).await {
            let mut group_handler = GroupBlockHandler::new();
            for event in closed.items {
                group_handler.handle_event(event).await?;
            }
            let groups = group_handler.collect();
            let runs_manager = RunsManager::default().await?;
            for group in groups {
                tracing::info!("Saving group: {:?}", group.id());
                runs_manager.upsert_run(group).await?;
            }
            drop(closed.sender); // Drop the sender to close the channel
        }
        Ok(())
    };
    let source_id = run_info.source_id.clone();

    TASK_MANAGER
        .spawn(task_id.clone(), async move |cancellation_token| {
            let run_fut = {
                let converted_run_index = run_index
                    .try_into()
                    .map_err(|e| tracing::error!("Failed to convert run_index to u32: {}", e))
                    .unwrap_or(0); // Default to 0 if conversion fails

                run_workflow_v2(
                    source_id,
                    topic_ref,
                    RetryStrategy::Retry {
                        replay_id,
                        run_index: converted_run_index,
                    },
                    None,
                )
            };
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    tracing::info!("Task {task_id} was cancelled");
                    if let Err(err) = callback_fn().await {
                        tracing::error!("Failed to handle callback for task {task_id}: {err}");
                    }
                }
                res = run_fut => {
                    match res {
                        Ok(_) => tracing::info!("Task {task_id} completed successfully"),
                        Err(err) => tracing::error!("Task {task_id} failed: {err}"),
                    }

                    if let Err(err) = callback_fn().await {
                        tracing::error!("Failed to handle callback for task {task_id}: {err}");
                    }
                }

            }
        })
        .await;

    Ok(extract::Json(CreateRunResponse { run: run_info }))
}

#[derive(serde::Deserialize, ToSchema, Debug)]
pub struct CancelRunRequest {
    pub source_id: String,
    pub run_index: u32,
}

#[utoipa::path(
    delete,
    path = "/runs/{source_id}/{run_index}",
    params(
        ("source_id" = String, Path, description = "SourceID for workflow is the path to the workflow file"),
        ("run_index" = i32, Path, description = "Run index")
    ),
    responses(
        (status = 200, description = "Successfully cancel the workflow run"),
        (status = 404, description = "Workflow not found"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Runs"
)]
pub async fn cancel_workflow_run(
    Path(payload): Path<CancelRunRequest>,
) -> Result<impl axum::response::IntoResponse, StatusCode> {
    let decoded_path = BASE64_STANDARD.decode(&payload.source_id).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let source_id = String::from_utf8(decoded_path).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let task_id = format!("{}::{}", source_id, &payload.run_index);
    TASK_MANAGER
        .cancel_task(task_id.clone())
        .await
        .map_err(|err| {
            tracing::error!("Failed to cancel task {task_id}: {err}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    tracing::info!("Cancelled task with ID: {}", task_id);
    Ok(())
}

#[derive(serde::Deserialize, ToSchema, Debug)]
pub struct WorkflowEventsRequest {
    pub source_id: String,
    pub run_index: i32,
}

impl From<WorkflowEventsRequest> for RunInfo {
    fn from(val: WorkflowEventsRequest) -> Self {
        RunInfo {
            source_id: val.source_id,
            run_index: Some(val.run_index),
            ..Default::default()
        }
    }
}

#[utoipa::path(
    method(get),
    path = "/events",
    params(
        ("source_id" = String, Query, description = "SourceID for workflow is the path to the workflow file"),
        ("run_index" = i32, Query, description = "Run index")
    ),
    responses(
        (status = OK, description = "Success", content_type = "text/event-stream")
    )
)]
pub async fn workflow_events(
    Query(request): Query<WorkflowEventsRequest>,
) -> Result<impl axum::response::IntoResponse, StatusCode> {
    let run_info: RunInfo = request.into();
    let runs_manager = RunsManager::default().await.map_err(|e| {
        tracing::error!("Failed to initialize RunsManager: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let run_info = runs_manager
        .find_run(&run_info.source_id, run_info.run_index)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find run: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    let run_info = match run_info.root_ref {
        Some(root_ref) => runs_manager
            .find_run(&root_ref.source_id, root_ref.run_index)
            .await
            .map_err(|e| {
                tracing::error!("Failed to find root run: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or(StatusCode::NOT_FOUND)?,
        None => run_info,
    };
    let run_id = run_info.task_id().map_err(|_| StatusCode::BAD_REQUEST)?;
    tracing::info!("Subscribing to events for run ID: {}", run_id);
    let subscribed = BROADCASTER.subscribe(&run_id).await.map_err(|err| {
        tracing::error!("Failed to subscribe to topic {run_id}: {err}");
        Into::<StatusCode>::into(err)
    })?;
    tracing::info!("Subscribed to events for run ID: {}", run_id);
    Ok(axum::response::sse::Sse::new(create_sse_broadcast(
        subscribed.items,
        subscribed.receiver,
    )))
}

#[derive(serde::Deserialize, ToSchema, Debug)]
pub struct BlocksRequest {
    pub source_id: String,
    pub run_index: Option<i32>,
}

#[utoipa::path(
    method(get),
    path = "/blocks",
    params(
        ("sourceId" = Option<String>, Path, description = "Combination of SourceID and RunIndex in RunInfo"),
        ("runIndex" = Option<String>, Query, description = "Run index to filter blocks")
    ),
    responses(
        (status = OK, description = "Success", body = RunDetails)
    )
)]
pub async fn get_blocks(
    Query(block_request): Query<BlocksRequest>,
) -> Result<extract::Json<RunDetails>, StatusCode> {
    let topic = match block_request.run_index {
        Some(run_index) => format!("{}::{}", block_request.source_id, run_index),
        None => block_request.source_id.clone(),
    };
    if BROADCASTER.has_topic(&topic).await {
        tracing::info!("Topic {} is running return empty blocks", topic);
        return Ok(extract::Json(RunDetails {
            run_info: RunInfo {
                source_id: block_request.source_id.clone(),
                run_index: block_request.run_index,
                ..Default::default()
            },
            blocks: None,
            children: None,
            error: None,
        }));
    }
    let run_details = RunsManager::default()
        .await?
        .find_run_details(&block_request.source_id, block_request.run_index)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get run details for {block_request:?}: {e:?}");
            StatusCode::BAD_REQUEST
        })?;
    match run_details {
        Some(details) => Ok(extract::Json(details)),
        None => {
            tracing::error!("Run details not found for {block_request:?}");
            Err(StatusCode::NOT_FOUND)
        }
    }
}
