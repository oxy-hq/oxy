use std::collections::HashSet;
use std::path::PathBuf;

use axum::extract::{self, Path, Query};
use axum::http::StatusCode;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::adapters::checkpoint::types::RetryStrategy;
use crate::api::middlewares::project::ProjectManagerExtractor;
use crate::auth::extractor::AuthenticatedUserExtractor;
use crate::errors::OxyError;
use crate::execute::types::OutputContainer;
use crate::execute::writer::Handler;
use crate::service::block::GroupBlockHandler;
use crate::service::statics::BROADCASTER;
use crate::service::task_manager::TASK_MANAGER;
use crate::service::types::pagination::{Paginated, Pagination};
use crate::service::types::run::{RunDetails, RunInfo, RunStatus};
use crate::service::workflow::run_workflow_v2;
use crate::utils::{create_sse_broadcast, file_path_to_source_id};

#[derive(serde::Deserialize, ToSchema)]
pub struct PaginationQuery {
    pub page: Option<usize>,
    pub size: Option<usize>,
}

/// Get paginated list of workflow runs
///
/// Retrieves all runs for a specific workflow with pagination support. Returns run metadata
/// including status, timestamps, and references. The status is updated in real-time by checking
/// if the workflow is currently active in the broadcaster.
#[utoipa::path(
    get,
    path = "/{project_id}/workflows/{pathb64}/runs",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("pathb64" = String, Path, description = "Base64 encoded path to the workflow"),
    ),
    params(
        ("page" = Option<usize>, Query, description = "Page number (default: 1)"),
        ("size" = Option<usize>, Query, description = "Items per page (default: 100, max: 100)")
    ),
    responses(
        (status = 200, description = "List of workflow runs with pagination", body = Paginated<RunInfo>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Runs"
)]
pub async fn get_workflow_runs(
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
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

    let mut result = project_manager
        .runs_manager
        .ok_or_else(|| {
            tracing::error!("Failed to initialize RunsManager");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
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
    pub replay_id: Option<String>,
}

#[derive(serde::Deserialize, ToSchema)]
pub struct CreateRunRequest {
    #[serde(flatten)]
    retry_strategy: RetryStrategy,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, ToSchema)]
pub struct CreateRunResponse {
    pub run: RunInfo,
}

/// Create and execute a new workflow run
///
/// Creates a new workflow run and immediately begins execution. Supports retry functionality
/// by providing a retry_param with run_id and optional replay_id. The workflow executes
/// asynchronously and broadcasts events via SSE. Returns the run information including
/// a unique task_id for tracking.
#[utoipa::path(
    post,
    path = "/{project_id}/workflows/{pathb64}/runs",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("pathb64" = String, Path, description = "Base64 encoded path to the workflow"),
    ),
    request_body = CreateRunRequest,
    responses(
        (status = 201, description = "Successfully create the workflow run", body = CreateRunResponse),
        (status = 404, description = "Workflow not found"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Runs"
)]
pub async fn create_workflow_run(
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
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

    let workflow_config = project_manager
        .config_manager
        .resolve_workflow(&path)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get workflow config: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let runs_manager = project_manager.runs_manager.clone().ok_or_else(|| {
        tracing::error!("Failed to initialize RunsManager");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let (source_run_info, root_run_info) = runs_manager
        .get_root_run(
            &file_path_to_source_id(&path),
            &payload.retry_strategy,
            None,
            Some(user.id),
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to get run info: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let replay_id = payload.retry_strategy.replay_id(&source_run_info.root_ref);
    let run_info = root_run_info.unwrap_or(source_run_info);

    tracing::info!("Creating new run {:?} with {:?}", run_info, replay_id);
    let task_id = run_info.task_id()?;
    let topic_ref = BROADCASTER.create_topic(&task_id).await.map_err(|err| {
        tracing::error!("Failed to create topic for task ID {task_id}: {err}");
        StatusCode::BAD_REQUEST
    })?;
    let run_index = run_info.run_index.ok_or(StatusCode::BAD_REQUEST)?;
    let topic_id = task_id.clone();
    let cb_source_id = run_info.source_id.clone();
    let cb_user_id = user.id; // Clone user.id for the callback
    let callback_fn = async move |output: Option<OutputContainer>| -> Result<(), OxyError> {
        // Handle the completion of the run and broadcast events
        if let Some(closed) = BROADCASTER.remove_topic(&topic_id).await {
            let last_task_ref = workflow_config.tasks.last().map(|t| t.name.clone());
            if let Some(output) = output
                && let Some(last_task_name) = last_task_ref
            {
                let outputs = output.find_ref(&last_task_name)?;
                let last_output = outputs.first();
                if let Some(last_output) = last_output {
                    tracing::info!(
                        "Final output for task '{}': {:?}",
                        last_task_name,
                        last_output
                    );
                    runs_manager
                        .update_run_output(
                            &cb_source_id,
                            run_index,
                            last_task_name,
                            last_output.to_json()?,
                        )
                        .await?;
                } else {
                    tracing::info!("No output found for the last task '{}'", last_task_name);
                }
            };

            let mut group_handler = GroupBlockHandler::new();
            for event in closed.items {
                group_handler.handle_event(event).await?;
            }
            let groups = group_handler.collect();
            for group in groups {
                tracing::info!("Saving group: {:?}", group.id());
                runs_manager.upsert_run(group, Some(cb_user_id)).await?;
            }
            drop(closed.sender); // Drop the sender to close the channel
        } else {
            tracing::warn!(
                "Failed to remove topic: {} - topic does not exist or was already removed",
                topic_id
            );
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
                    project_manager.clone(),
                    source_id,
                    topic_ref,
                    RetryStrategy::Retry {
                        replay_id,
                        run_index: converted_run_index,
                    },
                    None,
                    None,
                    None, // No globals for retry
                    user.id,
                )
            };
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    tracing::info!("Task {task_id} was cancelled");
                    if let Err(err) = callback_fn(None).await {
                        tracing::error!("Failed to handle callback for task {task_id}: {err}");
                    } else {
                        tracing::info!("Callback completed successfully for cancelled task {task_id}");
                    }
                }
                res = run_fut => {
                    let output = match res {
                        Ok(value) => {
                            tracing::info!("Task {task_id} completed successfully");
                            Some(value)
                        },
                        Err(err) => {
                            tracing::error!("Task {task_id} failed: {err}");
                            None
                        },
                    };
                    if let Err(err) = callback_fn(output).await {
                        tracing::error!("Failed to handle callback for task {task_id}: {err}");
                    } else {
                        tracing::info!("Callback completed successfully for task {task_id}");
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

/// Cancel a running workflow execution
///
/// Attempts to cancel an actively running workflow by its source_id and run_index.
/// The cancellation is handled by the task manager. Returns 200 on successful cancellation
/// or 500 if the task is not found or cancellation fails.
#[utoipa::path(
    delete,
    path = "/{project_id}/runs/{source_id}/{run_index}",
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
    security(
        ("ApiKey" = [])
    ),
    tag = "Runs"
)]
pub async fn cancel_workflow_run(
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    payload: Path<CancelRunRequest>,
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
    pub run_index: Option<i32>,
}

impl From<WorkflowEventsRequest> for RunInfo {
    fn from(val: WorkflowEventsRequest) -> Self {
        RunInfo {
            source_id: val.source_id,
            run_index: val.run_index,
            ..Default::default()
        }
    }
}

/// Subscribe to real-time workflow execution events via Server-Sent Events (SSE)
///
/// Establishes a persistent SSE connection to stream workflow execution events in real-time.
/// Returns all historical events first, then streams new events as they occur. Automatically
/// resolves to root workflow if the run is a child/retry run. If run_index is not specified,
/// attempts to get the latest run for the workflow.
#[utoipa::path(
    method(get),
    path = "/{project_id}/events",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("source_id" = String, Query, description = "SourceID for workflow is the path to the workflow file"),
        ("run_index" = Option<i32>, Query, description = "Run index (defaults to latest run if not specified)")
    ),
    responses(
        (status = OK, description = "Success", content_type = "text/event-stream")
    ),
    security(
        ("ApiKey" = [])
    )
)]
pub async fn workflow_events(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Query(request): Query<WorkflowEventsRequest>,
) -> Result<impl axum::response::IntoResponse, StatusCode> {
    let run_info: RunInfo = request.into();
    let runs_manager = project_manager.runs_manager.as_ref().ok_or_else(|| {
        tracing::error!("Failed to initialize RunsManager");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // If run_index is not specified, try to get the latest run
    let run_info = if run_info.run_index.is_none() {
        tracing::info!(
            "No run_index specified, attempting to get latest run for source_id: {}",
            run_info.source_id
        );
        runs_manager
            .last_run(&run_info.source_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get latest run: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or_else(|| {
                tracing::warn!("No runs found for source_id: {}", run_info.source_id);
                StatusCode::NOT_FOUND
            })?
    } else {
        runs_manager
            .find_run(&run_info.source_id, run_info.run_index)
            .await
            .map_err(|e| {
                tracing::error!("Failed to find run: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or(StatusCode::NOT_FOUND)?
    };
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

#[derive(serde::Serialize, ToSchema, Debug)]
pub struct WorkflowEventsResponse {
    pub events: Vec<serde_json::Value>,
    pub completed: bool,
}

/// Get all workflow events after completion (synchronous)
///
/// Returns all workflow execution events in a single response after the workflow completes.
/// If the workflow is still running, waits for completion and collects all events. For completed
/// workflows, retrieves stored blocks and converts them to events. Supports both base64-encoded
/// and plain source_id formats. If run_index is not specified, attempts to get the latest run.
#[utoipa::path(
    method(get),
    path = "/{project_id}/events/sync",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("source_id" = String, Query, description = "SourceID for workflow (file path or base64-encoded path)"),
        ("run_index" = Option<i32>, Query, description = "Run index (defaults to latest run if not specified)")
    ),
    responses(
        (status = 200, description = "All workflow events after completion", body = WorkflowEventsResponse),
        (status = 404, description = "Workflow run not found"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Runs"
)]
pub async fn workflow_events_sync(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Query(request): Query<WorkflowEventsRequest>,
) -> Result<axum::extract::Json<WorkflowEventsResponse>, StatusCode> {
    let source_id = if let Ok(decoded_bytes) = BASE64_STANDARD.decode(&request.source_id) {
        if let Ok(decoded_string) = String::from_utf8(decoded_bytes) {
            tracing::info!(
                "Decoded base64 source_id: {} -> {}",
                request.source_id,
                decoded_string
            );
            decoded_string
        } else {
            request.source_id.clone()
        }
    } else {
        request.source_id.clone()
    };

    let runs_manager = project_manager.runs_manager.as_ref().ok_or_else(|| {
        tracing::error!("Failed to initialize RunsManager");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::info!(
        "Looking for run: source_id={}, run_index={:?}",
        source_id,
        request.run_index
    );

    // If run_index is not specified, try to get the latest run
    let run_info = if request.run_index.is_none() {
        tracing::info!(
            "No run_index specified, attempting to get latest run for source_id: {}",
            source_id
        );
        runs_manager
            .last_run(&source_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get latest run: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or_else(|| {
                tracing::warn!("No runs found for source_id: {}", source_id);
                StatusCode::NOT_FOUND
            })?
    } else {
        runs_manager
            .find_run(&source_id, request.run_index)
            .await
            .map_err(|e| {
                tracing::error!("Failed to find run: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or_else(|| {
                tracing::warn!(
                    "Run not found: source_id={}, run_index={:?}",
                    source_id,
                    request.run_index
                );
                StatusCode::NOT_FOUND
            })?
    };

    let run_info = match run_info.root_ref {
        Some(root_ref) => {
            tracing::info!(
                "Found root reference, looking for root run: source_id={}, run_index={}",
                root_ref.source_id,
                root_ref.run_index.unwrap_or(0)
            );
            runs_manager
                .find_run(&root_ref.source_id, root_ref.run_index)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to find root run: {:?}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?
                .ok_or_else(|| {
                    tracing::warn!(
                        "Root run not found: source_id={}, run_index={}",
                        root_ref.source_id,
                        root_ref.run_index.unwrap_or(0)
                    );
                    StatusCode::NOT_FOUND
                })?
        }
        None => {
            tracing::info!("No root reference, using original run");
            run_info
        }
    };

    let run_id = run_info.task_id().map_err(|_| StatusCode::BAD_REQUEST)?;
    tracing::info!("Getting sync events for run ID: {}", run_id);

    let is_running = BROADCASTER.has_topic(&run_id).await;

    if is_running {
        let subscribed = BROADCASTER.subscribe(&run_id).await.map_err(|err| {
            tracing::error!("Failed to subscribe to topic {run_id}: {err}");
            Into::<StatusCode>::into(err)
        })?;

        let mut all_events = Vec::new();

        for event in subscribed.items {
            if let Ok(event_json) = serde_json::to_value(&event) {
                all_events.push(event_json);
            }
        }

        let mut receiver = subscribed.receiver;

        while let Ok(event) = receiver.recv().await {
            if let Ok(event_json) = serde_json::to_value(&event) {
                all_events.push(event_json);
            }
        }

        tracing::info!(
            "Collected {} events for run ID: {}",
            all_events.len(),
            run_id
        );

        Ok(axum::extract::Json(WorkflowEventsResponse {
            events: all_events,
            completed: true,
        }))
    } else {
        tracing::info!(
            "Run ID {} is not active, attempting to get stored run details",
            run_id
        );

        let run_details = runs_manager
            .find_run_details(&run_info.source_id, run_info.run_index)
            .await
            .map_err(|e| {
                tracing::warn!("Failed to get run details: {:?}", e);
            })
            .unwrap_or(None);

        let events = if let Some(details) = run_details {
            if let Some(blocks) = details.blocks {
                tracing::info!("Found {} blocks for completed run", blocks.len());
                blocks
                    .into_iter()
                    .enumerate()
                    .map(|(i, block)| {
                        serde_json::json!({
                            "type": "block_execution",
                            "index": i,
                            "block": block,
                            "timestamp": chrono::Utc::now().to_rfc3339()
                        })
                    })
                    .collect()
            } else {
                tracing::info!("No blocks found for completed run");
                vec![]
            }
        } else {
            tracing::info!("No run details found for completed run");
            vec![]
        };

        tracing::info!(
            "Returning {} events for completed run ID: {}",
            events.len(),
            run_id
        );

        Ok(axum::extract::Json(WorkflowEventsResponse {
            events,
            completed: true,
        }))
    }
}

#[derive(serde::Deserialize, ToSchema, Debug)]
pub struct BlocksRequest {
    pub source_id: String,
    pub run_index: Option<i32>,
}

/// Get execution blocks and details for a workflow run
///
/// Retrieves detailed execution information including blocks, children runs, and errors
/// for a specific workflow run. Returns empty blocks if the workflow is currently running
/// to avoid incomplete data. If run_index is not specified, attempts to get the latest run.
#[utoipa::path(
    method(get),
    path = "/{project_id}/blocks",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("sourceId" = Option<String>, Path, description = "Combination of SourceID and RunIndex in RunInfo"),
        ("runIndex" = Option<String>, Query, description = "Run index to filter blocks (defaults to latest run if not specified)")
    ),
    params(
        ("sourceId" = Option<String>, Path, description = "Combination of SourceID and RunIndex in RunInfo"),
        ("runIndex" = Option<String>, Query, description = "Run index to filter blocks (defaults to latest run if not specified)")
    ),
    responses(
        (status = OK, description = "Success", body = RunDetails)
    ),
    security(
        ("ApiKey" = [])
    )
)]
pub async fn get_blocks(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Query(block_request): Query<BlocksRequest>,
) -> Result<extract::Json<RunDetails>, StatusCode> {
    let runs_manager = project_manager.runs_manager.as_ref().ok_or_else(|| {
        tracing::error!("Failed to initialize RunsManager");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // If run_index is not specified, try to get the latest run
    let run_index = if block_request.run_index.is_none() {
        tracing::info!(
            "No run_index specified, attempting to get latest run for source_id: {}",
            block_request.source_id
        );
        let latest_run = runs_manager
            .last_run(&block_request.source_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get latest run: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        latest_run.and_then(|r| r.run_index)
    } else {
        block_request.run_index
    };

    let topic = match run_index {
        Some(idx) => format!("{}::{}", block_request.source_id, idx),
        None => block_request.source_id.clone(),
    };
    if BROADCASTER.has_topic(&topic).await {
        tracing::info!("Topic {} is running return empty blocks", topic);
        return Ok(extract::Json(RunDetails {
            run_info: RunInfo {
                source_id: block_request.source_id.clone(),
                run_index,
                ..Default::default()
            },
            blocks: None,
            children: None,
            error: None,
            output: None,
        }));
    }

    let run_details = runs_manager
        .find_run_details(&block_request.source_id, run_index)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to get run details for source_id={}, run_index={:?}: {e:?}",
                block_request.source_id,
                run_index
            );
            StatusCode::BAD_REQUEST
        })?;
    match run_details {
        Some(details) => Ok(extract::Json(details)),
        None => {
            tracing::error!(
                "Run details not found for source_id={}, run_index={:?}",
                block_request.source_id,
                run_index
            );
            Err(StatusCode::NOT_FOUND)
        }
    }
}

/// Delete a workflow run from the database
///
/// Permanently removes a workflow run record from the database. This operation cannot be undone.
/// The run is identified by the workflow path (base64 encoded) and run index.
#[utoipa::path(
    delete,
    path = "/{project_id}/workflows/{pathb64}/runs/{run_id}",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("pathb64" = String, Path, description = "Base64 encoded path to the workflow"),
        ("run_id" = i32, Path, description = "Run index to delete")
    ),
    responses(
        (status = 200, description = "Successfully deleted the workflow run"),
        (status = 404, description = "Workflow run not found"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Runs"
)]
pub async fn delete_workflow_run(
    Path((_project_id, pathb64, run_id)): Path<(Uuid, String, i32)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
) -> Result<impl axum::response::IntoResponse, StatusCode> {
    let decoded_path = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        tracing::error!("Failed to decode path: {:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let path: PathBuf = PathBuf::from(String::from_utf8(decoded_path).map_err(|e| {
        tracing::error!("Failed to convert path to UTF-8: {:?}", e);
        StatusCode::BAD_REQUEST
    })?);

    let runs_manager = project_manager.runs_manager.ok_or_else(|| {
        tracing::error!("Failed to initialize RunsManager");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let source_id = file_path_to_source_id(&path);
    runs_manager
        .delete_run(&source_id, run_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete run: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    tracing::info!(
        "Successfully deleted run for source_id: {}, run_index: {}",
        source_id,
        run_id
    );
    Ok(())
}

#[derive(serde::Deserialize, ToSchema, Debug)]
pub struct BulkDeleteRunsRequest {
    pub runs: Vec<RunIdentifier>,
}

#[derive(serde::Deserialize, ToSchema, Debug)]
pub struct RunIdentifier {
    pub pathb64: String,
    pub run_index: i32,
}

#[derive(serde::Serialize, ToSchema, Debug)]
pub struct BulkDeleteRunsResponse {
    pub deleted_count: u64,
}

/// Bulk delete multiple workflow runs from the database
///
/// Permanently removes multiple workflow run records from the database in a single operation.
/// This operation cannot be undone. Each run is identified by its workflow path (base64 encoded)
/// and run index.
#[utoipa::path(
    post,
    path = "/{project_id}/workflows/runs/bulk-delete",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
    ),
    request_body = BulkDeleteRunsRequest,
    responses(
        (status = 200, description = "Successfully deleted workflow runs", body = BulkDeleteRunsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Runs"
)]
pub async fn bulk_delete_workflow_runs(
    Path(_project_id): Path<Uuid>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    extract::Json(payload): extract::Json<BulkDeleteRunsRequest>,
) -> Result<extract::Json<BulkDeleteRunsResponse>, StatusCode> {
    let runs_manager = project_manager.runs_manager.ok_or_else(|| {
        tracing::error!("Failed to initialize RunsManager");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut run_ids = Vec::new();
    for run_identifier in payload.runs {
        let decoded_path = BASE64_STANDARD
            .decode(&run_identifier.pathb64)
            .map_err(|e| {
                tracing::error!("Failed to decode path: {:?}", e);
                StatusCode::BAD_REQUEST
            })?;
        let path: PathBuf = PathBuf::from(String::from_utf8(decoded_path).map_err(|e| {
            tracing::error!("Failed to convert path to UTF-8: {:?}", e);
            StatusCode::BAD_REQUEST
        })?);
        let source_id = file_path_to_source_id(&path);
        run_ids.push((source_id, run_identifier.run_index));
    }

    let deleted_count = runs_manager.bulk_delete_runs(run_ids).await.map_err(|e| {
        tracing::error!("Failed to bulk delete runs: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::info!("Successfully deleted {} runs", deleted_count);
    Ok(extract::Json(BulkDeleteRunsResponse { deleted_count }))
}
