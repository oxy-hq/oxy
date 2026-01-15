use crate::server::api::middlewares::project::ProjectManagerExtractor;
use crate::server::router::ProjectExtractor;
use crate::server::service::task_manager::TASK_MANAGER;
use axum::{
    extract::{self, Path, Query},
    http::StatusCode,
};
use entity::logs;
use entity::prelude::Logs;
use entity::prelude::Threads;
use entity::threads;
use oxy::{
    database::client::establish_connection,
    execute::types::{ReferenceKind, event::SandboxInfo},
};
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, prelude::DateTimeWithTimeZone,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Serialize, ToSchema)]
pub struct ThreadItem {
    pub id: String,
    pub title: String,
    pub input: String,
    pub output: String,
    pub source_type: String,
    pub source: String,
    pub created_at: DateTimeWithTimeZone,
    pub references: Vec<ReferenceKind>,
    pub sandbox_info: Option<SandboxInfo>,
    pub is_processing: bool,
}

#[derive(Serialize, ToSchema)]
pub struct ThreadsResponse {
    pub threads: Vec<ThreadItem>,
    pub pagination: PaginationInfo,
}

#[derive(Serialize, ToSchema)]
pub struct PaginationInfo {
    pub page: u64,
    pub limit: u64,
    pub total: u64,
    pub total_pages: u64,
    pub has_next: bool,
    pub has_previous: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct PaginationQuery {
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateThreadRequest {
    pub title: String,
    pub input: String,
    pub source: String,
    pub source_type: String,
}

/// Get paginated list of threads for the authenticated user
#[utoipa::path(
    get,
    path = "/{project_id}/threads",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("page" = Option<u64>, Query, description = "Page number (default: 1)"),
        ("limit" = Option<u64>, Query, description = "Items per page (default: 100, max: 100)")
    ),
    responses(
        (status = 200, description = "List of threads with pagination", body = ThreadsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Threads"
)]
pub async fn get_threads(
    ProjectExtractor(project): ProjectExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Query(pagination): Query<PaginationQuery>,
) -> Result<extract::Json<ThreadsResponse>, StatusCode> {
    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let page = pagination.page.unwrap_or(1);
    let limit = pagination.limit.unwrap_or(100).clamp(1, 100);
    let page = page.max(1);
    let total = Threads::find()
        .filter(threads::Column::UserId.eq(Some(user.id)))
        .filter(threads::Column::ProjectId.eq(project.id))
        .count(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if total == 0 {
        return Ok(extract::Json(ThreadsResponse {
            threads: vec![],
            pagination: PaginationInfo {
                page,
                limit,
                total: 0,
                total_pages: 0,
                has_next: false,
                has_previous: false,
            },
        }));
    }

    let total_pages = total.div_ceil(limit); // Ceiling division for pagination
    let has_next = page < total_pages;
    let has_previous = page > 1;
    let offset = (page - 1) * limit;

    let threads = Threads::find()
        .filter(threads::Column::UserId.eq(Some(user.id)))
        .filter(threads::Column::ProjectId.eq(project.id))
        .order_by_desc(threads::Column::CreatedAt)
        .offset(offset)
        .limit(limit)
        .all(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let thread_items = threads
        .into_iter()
        .map(|t| ThreadItem {
            id: t.id.to_string(),
            title: t.title.clone(),
            input: t.input.clone(),
            output: t.output.clone(),
            source: t.source.clone(),
            source_type: t.source_type.clone(),
            created_at: t.created_at,
            references: serde_json::from_str(&t.references).unwrap_or_default(),
            sandbox_info: t
                .sandbox_info
                .as_ref()
                .and_then(|info| serde_json::from_value(info.clone()).ok()),
            is_processing: t.is_processing,
        })
        .collect();

    Ok(extract::Json(ThreadsResponse {
        threads: thread_items,
        pagination: PaginationInfo {
            page,
            limit,
            total,
            total_pages,
            has_next,
            has_previous,
        },
    }))
}

/// Get a specific thread by ID
///
/// Retrieves detailed information about a single thread including its input, output,
/// references, and processing status. The thread must belong to the authenticated user.
#[utoipa::path(
    get,
    path = "/{project_id}/threads/{id}",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("id" = String, Path, description = "Thread ID (UUID)")
    ),
    responses(
        (status = 200, description = "Thread details", body = ThreadItem),
        (status = 400, description = "Invalid thread ID format"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Thread not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Threads"
)]
pub async fn get_thread(
    Path((_project_id, id)): Path<(Uuid, String)>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<extract::Json<ThreadItem>, StatusCode> {
    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let thread_id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;

    let thread = Threads::find_by_id(thread_id)
        .filter(threads::Column::UserId.eq(Some(user.id)))
        .one(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let thread_item = ThreadItem {
        id: thread.id.to_string(),
        title: thread.title,
        input: thread.input,
        output: thread.output,
        source_type: thread.source_type,
        source: thread.source,
        created_at: thread.created_at,
        references: serde_json::from_str(&thread.references).unwrap_or_default(),
        sandbox_info: thread
            .sandbox_info
            .as_ref()
            .and_then(|info| serde_json::from_value(info.clone()).ok()),
        is_processing: thread.is_processing,
    };
    Ok(extract::Json(thread_item))
}

/// Create a new thread
#[utoipa::path(
    post,
    path = "/{project_id}/threads",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID")
    ),
    request_body = CreateThreadRequest,
    responses(
        (status = 200, description = "Thread created successfully", body = ThreadItem),
        (status = 400, description = "Invalid request data"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Threads"
)]
pub async fn create_thread(
    ProjectExtractor(project): ProjectExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    extract::Json(thread_request): extract::Json<CreateThreadRequest>,
) -> Result<extract::Json<ThreadItem>, StatusCode> {
    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let new_thread = entity::threads::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        user_id: ActiveValue::Set(Some(user.id)),
        created_at: ActiveValue::not_set(),
        title: ActiveValue::Set(thread_request.title),
        input: ActiveValue::Set(thread_request.input.clone()),
        output: ActiveValue::Set("".to_string()),
        source_type: ActiveValue::Set(thread_request.source_type),
        source: ActiveValue::Set(thread_request.source),
        references: ActiveValue::Set("[]".to_string()),
        is_processing: ActiveValue::Set(false),
        project_id: ActiveValue::Set(project.id),
        sandbox_info: ActiveValue::Set(None),
    };
    let thread = new_thread
        .insert(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let thread_item = ThreadItem {
        id: thread.id.to_string(),
        title: thread.title,
        input: thread.input,
        output: thread.output,
        source_type: thread.source_type,
        source: thread.source,
        created_at: thread.created_at,
        references: serde_json::from_str(&thread.references).unwrap_or_default(),
        sandbox_info: thread
            .sandbox_info
            .as_ref()
            .and_then(|info| serde_json::from_value(info.clone()).ok()),
        is_processing: thread.is_processing,
    };
    Ok(extract::Json(thread_item))
}

/// Delete a specific thread
#[utoipa::path(
    delete,
    path = "/{project_id}/threads/{id}",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("id" = String, Path, description = "Thread ID (UUID)")
    ),
    responses(
        (status = 200, description = "Thread deleted successfully"),
        (status = 400, description = "Invalid thread ID format"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Thread not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Threads"
)]
pub async fn delete_thread(
    Path((_project_id, id)): Path<(Uuid, String)>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<StatusCode, StatusCode> {
    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let thread_id = Uuid::parse_str(&id).map_err(|e| {
        tracing::warn!("Invalid thread ID format '{}': {}", id, e);
        StatusCode::BAD_REQUEST
    })?;

    let thread = Threads::find_by_id(thread_id)
        .filter(threads::Column::UserId.eq(Some(user.id)))
        .one(&connection)
        .await
        .map_err(|e| {
            tracing::error!(
                "Database error finding thread {} for deletion by user {}: {}",
                thread_id,
                user.id,
                e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if let Some(thread) = thread {
        // Check if thread is being processed
        if thread.is_processing {
            tracing::warn!(
                "Attempted to delete thread {} that is currently being processed",
                thread_id
            );
            return Err(StatusCode::CONFLICT);
        }

        let active_thread: entity::threads::ActiveModel = thread.into();
        active_thread.delete(&connection).await.map_err(|e| {
            tracing::error!(
                "Failed to delete thread {} for user {}: {}",
                thread_id,
                user.id,
                e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        tracing::info!(
            "Successfully deleted thread {} for user {}",
            thread_id,
            user.id
        );
    } else {
        tracing::warn!(
            "Thread {} not found or doesn't belong to user {}",
            thread_id,
            user.id
        );
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::OK)
}

fn remove_all_files_in_dir<P: AsRef<std::path::Path>>(dir: P) {
    if dir.as_ref().exists()
        && let Ok(entries) = std::fs::read_dir(&dir)
    {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

/// Delete all threads for the authenticated user
#[utoipa::path(
    delete,
    path = "/{project_id}/threads",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID")
    ),
    responses(
        (status = 200, description = "All threads deleted successfully"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Threads"
)]
pub async fn delete_all_threads(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    ProjectExtractor(project): ProjectExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<StatusCode, StatusCode> {
    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Threads::delete_many()
        .filter(threads::Column::UserId.eq(Some(user.id)))
        .filter(threads::Column::ProjectId.eq(project.id))
        .exec(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Note: Only removing charts for this user would require more complex logic
    // For now, we'll keep the current behavior but you may want to change this
    {
        let charts_dir = project_manager.config_manager.get_charts_dir().await?;
        remove_all_files_in_dir(charts_dir);
    }

    Ok(StatusCode::OK)
}

/// Stop a running thread
#[utoipa::path(
    post,
    path = "/{project_id}/threads/{id}/stop",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("id" = String, Path, description = "Thread ID (UUID)")
    ),
    responses(
        (status = 200, description = "Thread stopped successfully"),
        (status = 400, description = "Invalid thread ID format"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Thread not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Threads"
)]
pub async fn stop_thread(
    Path((_project_id, id)): Path<(Uuid, String)>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<StatusCode, StatusCode> {
    let thread_id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Verify the user owns this thread
    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let thread = Threads::find_by_id(thread_id)
        .filter(threads::Column::UserId.eq(Some(user.id)))
        .one(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if thread.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    let thread = thread.unwrap();
    let mut thread_model: entity::threads::ActiveModel = thread.clone().into();
    thread_model.is_processing = ActiveValue::Set(false);

    if let Err(e) = thread_model.update(&connection).await {
        tracing::error!(
            "Failed to unlock thread {} during stop operation: {}",
            thread.id,
            e
        );
        // Continue with cancellation even if update fails
    }

    match TASK_MANAGER.cancel_task(thread_id).await {
        Ok(true) => {
            TASK_MANAGER.remove_task(thread_id).await;
            tracing::info!("Successfully stopped and unlocked thread {}", thread_id);
            Ok(StatusCode::OK)
        }
        Ok(false) => {
            tracing::warn!("Task not found for thread {}", thread_id);
            Err(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            tracing::error!("Error cancelling task for thread {}: {}", thread_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct BulkDeleteThreadsRequest {
    pub thread_ids: Vec<String>,
}

/// Bulk delete multiple threads by their IDs
///
/// Efficiently deletes multiple threads in a single operation. All threads must belong
/// to the authenticated user. Invalid UUIDs will result in a 400 Bad Request error.
/// Empty thread_ids array will also return 400 Bad Request.
#[utoipa::path(
    post,
    path = "/{project_id}/threads/bulk-delete",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID")
    ),
    request_body = BulkDeleteThreadsRequest,
    responses(
        (status = 200, description = "Threads deleted successfully"),
        (status = 400, description = "Invalid request data or thread IDs"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Threads"
)]
pub async fn bulk_delete_threads(
    ProjectExtractor(project): ProjectExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    extract::Json(request): extract::Json<BulkDeleteThreadsRequest>,
) -> Result<StatusCode, StatusCode> {
    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut thread_uuids = Vec::new();
    for thread_id in request.thread_ids {
        let uuid = Uuid::parse_str(&thread_id).map_err(|_| StatusCode::BAD_REQUEST)?;
        thread_uuids.push(uuid);
    }

    if thread_uuids.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    Threads::delete_many()
        .filter(
            threads::Column::UserId
                .eq(Some(user.id))
                .and(threads::Column::ProjectId.eq(project.id))
                .and(threads::Column::Id.is_in(thread_uuids)),
        )
        .exec(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

#[derive(Serialize, ToSchema)]
pub struct LogItem {
    pub id: String,
    pub user_id: String,
    pub prompts: String,
    pub thread_id: String,
    pub log: serde_json::Value,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub thread: Option<ThreadInfo>,
}

#[derive(Serialize, ToSchema)]
pub struct ThreadInfo {
    pub title: String,
    pub input: String,
    pub output: String,
    pub source: String,
    pub source_type: String,
    pub is_processing: bool,
}

#[derive(Serialize, ToSchema)]
pub struct LogsResponse {
    pub logs: Vec<LogItem>,
}

/// Get execution logs with associated thread information
///
/// Retrieves all execution logs for the authenticated user, including associated thread
/// details such as title, input, output, and processing status. Logs are ordered by
/// creation time in descending order (most recent first).
#[utoipa::path(
    get,
    path = "/logs",
    responses(
        (status = 200, description = "List of logs with thread information", body = LogsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Threads"
)]
pub async fn get_logs(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<extract::Json<LogsResponse>, StatusCode> {
    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let logs_with_threads = Logs::find()
        .filter(logs::Column::UserId.eq(user.id))
        .find_also_related(Threads)
        .order_by_desc(logs::Column::CreatedAt)
        .all(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let log_items = logs_with_threads
        .into_iter()
        .map(|(log, thread)| LogItem {
            id: log.id.to_string(),
            user_id: log.user_id.to_string(),
            prompts: log.prompts,
            thread_id: log.thread_id.to_string(),
            log: log.log,
            created_at: log.created_at,
            updated_at: log.updated_at,
            thread: thread.map(|t| ThreadInfo {
                title: t.title,
                input: t.input,
                output: t.output,
                source: t.source,
                source_type: t.source_type,
                is_processing: t.is_processing,
            }),
        })
        .collect();

    Ok(extract::Json(LogsResponse { logs: log_items }))
}
