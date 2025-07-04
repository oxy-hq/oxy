use crate::{
    auth::extractor::AuthenticatedUserExtractor,
    db::client::establish_connection,
    errors::OxyError,
    execute::types::ReferenceKind,
    service::{
        agent::{Message, run_agent},
        formatters::{BlockHandler, streaming_message_persister::StreamingMessagePersister},
        types::{AnswerContent, AnswerStream},
    },
    utils::{create_sse_stream, find_project_path},
};
use axum::{
    extract::{self, Path, Query},
    http::StatusCode,
    response::IntoResponse,
    response::sse::Sse,
};
use entity::threads;
use entity::{prelude::Messages, prelude::Threads};
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, Condition, EntityTrait, Order, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, prelude::DateTimeWithTimeZone,
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
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
    let connection = establish_connection().await;

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
    let connection = establish_connection().await;
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
    let connection = establish_connection().await;
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
    let connection = establish_connection().await;
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
    let connection = establish_connection().await;
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

#[derive(Deserialize)]
pub struct AskThreadRequest {
    pub question: Option<String>,
}

pub async fn ask_thread(
    Path(id): Path<String>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    extract::Json(payload): extract::Json<AskThreadRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let connection = establish_connection().await;
    let thread_id = Uuid::parse_str(&id).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;

    let thread = Threads::find_by_id(thread_id)
        .filter(threads::Column::UserId.eq(Some(user.id)))
        .one(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let thread = thread.ok_or(StatusCode::NOT_FOUND)?;

    if thread.is_processing {
        return Err(StatusCode::CONFLICT);
    }

    let mut thread_model: entity::threads::ActiveModel = thread.clone().into();
    thread_model.is_processing = ActiveValue::Set(true);
    thread_model
        .update(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut messages = Messages::find()
        .filter(
            Condition::all()
                .add(<entity::prelude::Messages as EntityTrait>::Column::ThreadId.eq(thread.id)),
        )
        .order_by(
            <entity::prelude::Messages as EntityTrait>::Column::CreatedAt,
            Order::Desc,
        )
        .limit(10)
        .all(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // sort the 10 most recent messages by created_at asc
    messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    let user_question;
    match payload.question {
        Some(question) => {
            user_question = question.clone();
            let message = entity::messages::ActiveModel {
                id: ActiveValue::Set(Uuid::new_v4()),
                content: ActiveValue::Set(question),
                is_human: ActiveValue::Set(true),
                thread_id: ActiveValue::Set(thread.id),
                created_at: ActiveValue::default(),
                ..Default::default()
            };
            message
                .insert(&connection)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        None => {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let memory = messages
        .into_iter()
        .map(|message| Message {
            content: message.content,
            is_human: message.is_human,
            created_at: message.created_at,
        })
        .collect::<Vec<Message>>();

    let project_path = find_project_path().map_err(|e| {
        tracing::info!("Failed to find project path: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let agent_ref = thread.source.to_string();
    let (tx, rx) = tokio::sync::mpsc::channel(100);

    let _ = tokio::spawn(async move {
        let tx_clone = tx.clone();
        let streaming_message_persister = Arc::new(
            StreamingMessagePersister::new(connection.clone(), thread.id, "".to_owned())
                .await
                .map_err(|err| {
                    OxyError::DBError(format!("Failed to create streaming message handler: {err}"))
                })?,
        );

        let block_handler =
            BlockHandler::new(tx).with_streaming_persister(streaming_message_persister.clone());
        let block_handler_reader = block_handler.get_reader();
        let result = run_agent(
            &project_path,
            &PathBuf::from(agent_ref),
            user_question,
            block_handler,
            memory,
        )
        .await;
        println!("Running agent with question: {result:?}");
        match result {
            Ok(output_container) => {
                tracing::debug!("Agent output: {:?}", output_container);
                let (mut answer_message, artifacts) =
                    block_handler_reader.into_active_models().await?;
                answer_message.thread_id = ActiveValue::Set(thread.id);
                answer_message.id = ActiveValue::Set(streaming_message_persister.get_message_id());

                let message_model = answer_message.update(&connection).await.map_err(|err| {
                    OxyError::DBError(format!("Failed to insert message:\n{err}"))
                })?;
                println!("Updated message: {message_model:?}");

                for mut artifact in artifacts {
                    artifact.thread_id = ActiveValue::Set(thread.id);
                    artifact.message_id = ActiveValue::Set(message_model.id);
                    println!("Inserting artifact: {artifact:?}");
                    let response = artifact.insert(&connection).await.map_err(|err| {
                        println!("Failed to insert artifact: {err}");
                        OxyError::DBError(format!("Failed to insert artifact:\n{err}"))
                    })?;
                    println!("Inserted artifact: {response:?}");
                }
                let mut thread_model: entity::threads::ActiveModel = thread.into();
                thread_model.is_processing = ActiveValue::Set(false);
                let _ = thread_model.update(&connection).await;

                Result::<(), OxyError>::Ok(())
            }
            Err(err) => {
                tracing::error!("Error running agent: {}", err);

                let msg = format!("🔴 Error: {err}");

                // Fallback: create error message normally
                let answer_message = entity::messages::ActiveModel {
                    id: ActiveValue::Set(streaming_message_persister.get_message_id()),
                    content: ActiveValue::Set(msg.clone()),
                    is_human: ActiveValue::Set(false),
                    thread_id: ActiveValue::Set(thread.id),
                    created_at: ActiveValue::default(),
                    input_tokens: ActiveValue::Set(0),
                    output_tokens: ActiveValue::Set(0),
                };

                let _ = answer_message.update(&connection).await.map_err(|err| {
                    OxyError::DBError(format!("Failed to insert message:\n{err}"))
                })?;

                let error_event = AnswerStream {
                    content: AnswerContent::Error { message: msg },
                    references: vec![],
                    is_error: true,
                    step: "".to_string(),
                };
                tx_clone
                    .send(error_event)
                    .await
                    .map_err(|_| OxyError::RuntimeError("Failed to send message".to_string()))?;

                let mut thread_model: entity::threads::ActiveModel = thread.into();
                thread_model.is_processing = ActiveValue::Set(false);
                let _ = thread_model.update(&connection).await;

                Result::<(), OxyError>::Ok(())
            }
        }
    });

    Ok(Sse::new(create_sse_stream(rx)))
}

#[derive(Deserialize)]
pub struct BulkDeleteThreadsRequest {
    pub thread_ids: Vec<String>,
}

pub async fn bulk_delete_threads(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    extract::Json(request): extract::Json<BulkDeleteThreadsRequest>,
) -> Result<StatusCode, StatusCode> {
    let connection = establish_connection().await;

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
