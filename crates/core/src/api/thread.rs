use crate::{
    auth::extractor::AuthenticatedUserExtractor, db::client::establish_connection,
    execute::types::ReferenceKind, service::task_manager::TASK_MANAGER,
};
use axum::{
    extract::{self, Path, Query},
    http::StatusCode,
};
use entity::logs;
use entity::prelude::Logs;
use entity::prelude::Threads;
use entity::threads;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, prelude::DateTimeWithTimeZone,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize)]
pub struct ThreadItem {
    pub id: String,
    pub title: String,
    pub input: String,
    pub output: String,
    pub source_type: String,
    pub source: String,
    pub created_at: DateTimeWithTimeZone,
    pub references: Vec<ReferenceKind>,
    pub is_processing: bool,
}

#[derive(Serialize)]
pub struct ThreadsResponse {
    pub threads: Vec<ThreadItem>,
    pub pagination: PaginationInfo,
}

#[derive(Serialize)]
pub struct PaginationInfo {
    pub page: u64,
    pub limit: u64,
    pub total: u64,
    pub total_pages: u64,
    pub has_next: bool,
    pub has_previous: bool,
}

#[derive(Deserialize)]
pub struct PaginationQuery {
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

#[derive(Deserialize)]
pub struct CreateThreadRequest {
    pub title: String,
    pub input: String,
    pub source: String,
    pub source_type: String,
}

pub async fn get_threads(
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

pub async fn get_thread(
    Path(id): Path<String>,
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
        is_processing: thread.is_processing,
    };
    Ok(extract::Json(thread_item))
}

pub async fn create_thread(
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
        is_processing: thread.is_processing,
    };
    Ok(extract::Json(thread_item))
}

pub async fn delete_thread(
    Path(id): Path<String>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<StatusCode, StatusCode> {
    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let thread_id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;

    let thread = Threads::find_by_id(thread_id)
        .filter(threads::Column::UserId.eq(Some(user.id)))
        .one(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(thread) = thread {
        let active_thread: entity::threads::ActiveModel = thread.into();
        active_thread
            .delete(&connection)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    } else {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::OK)
}

fn remove_all_files_in_dir<P: AsRef<std::path::Path>>(dir: P) {
    if dir.as_ref().exists() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
    }
}

pub async fn delete_all_threads(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<StatusCode, StatusCode> {
    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Threads::delete_many()
        .filter(threads::Column::UserId.eq(Some(user.id)))
        .exec(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Note: Only removing charts for this user would require more complex logic
    // For now, we'll keep the current behavior but you may want to change this
    {
        use crate::db::client::get_charts_dir;
        remove_all_files_in_dir(get_charts_dir());
    }

    Ok(StatusCode::OK)
}

pub async fn stop_thread(
    Path(id): Path<String>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<StatusCode, StatusCode> {
    let thread_id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Verify the user owns this thread
    let connection = establish_connection().await?;
    let thread = Threads::find_by_id(thread_id)
        .filter(threads::Column::UserId.eq(Some(user.id)))
        .one(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if thread.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    let mut thread_model: entity::threads::ActiveModel = thread.unwrap().into();
    thread_model.is_processing = ActiveValue::Set(false);
    let _ = thread_model.update(&connection).await;

    match TASK_MANAGER.cancel_task(thread_id).await {
        Ok(true) => {
            TASK_MANAGER.remove_task(thread_id).await;

            Ok(StatusCode::OK)
        }
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[derive(Deserialize)]
pub struct BulkDeleteThreadsRequest {
    pub thread_ids: Vec<String>,
}

pub async fn bulk_delete_threads(
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
                .and(threads::Column::Id.is_in(thread_uuids)),
        )
        .exec(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

#[derive(Serialize)]
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

#[derive(Serialize)]
pub struct ThreadInfo {
    pub title: String,
    pub input: String,
    pub output: String,
    pub source: String,
    pub source_type: String,
    pub is_processing: bool,
}

#[derive(Serialize)]
pub struct LogsResponse {
    pub logs: Vec<LogItem>,
}

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
