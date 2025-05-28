use crate::{
    auth::extractor::AuthenticatedUserExtractor,
    db::client::establish_connection,
    errors::OxyError,
    execute::{
        types::{Event, ReferenceKind},
        writer::{EventHandler, MarkdownWriter, OutputWriter},
    },
    service::agent::{Message, run_agent},
    utils::{find_project_path, try_unwrap_arc_mutex, try_unwrap_arc_tokio_mutex},
};
use axum::{
    extract::{self, Path, Query},
    http::StatusCode,
    response::IntoResponse,
};
use axum_streams::StreamBodyAs;
use base64::{Engine, prelude::BASE64_STANDARD};
use entity::threads;
use entity::{prelude::Messages, prelude::Threads};
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, Condition, EntityTrait, Order, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, prelude::DateTimeWithTimeZone,
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;
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

#[derive(Serialize)]
pub struct AnswerStream {
    pub content: String,
    pub references: Vec<ReferenceKind>,
    pub is_error: bool,
    pub step: String,
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
    };

    let message = entity::messages::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        content: ActiveValue::Set(thread_request.input),
        is_human: ActiveValue::Set(true),
        thread_id: ActiveValue::Set(thread.id),
        created_at: ActiveValue::default(),
    };
    message.insert(&connection).await.map_err(|e| {
        tracing::error!("Failed to insert message: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
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

struct ThreadStream {
    references: Arc<Mutex<Vec<ReferenceKind>>>,
    tx: Sender<AnswerStream>,
    output_writer: Arc<tokio::sync::Mutex<MarkdownWriter>>,
}

impl ThreadStream {
    fn new(
        tx: Sender<AnswerStream>,
        references: Arc<Mutex<Vec<ReferenceKind>>>,
        writer: Arc<tokio::sync::Mutex<MarkdownWriter>>,
    ) -> Self {
        ThreadStream {
            tx,
            references,
            output_writer: writer,
        }
    }
}

#[async_trait::async_trait]
impl EventHandler for ThreadStream {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        let mut output_writer = self.output_writer.lock().await;
        let event_format = output_writer.write_event(&event).await?;

        if let Some(event_format) = event_format {
            self.tx
                .send(AnswerStream {
                    content: event_format.content,
                    references: match event_format.reference {
                        Some(reference) => {
                            self.references.lock().unwrap().push(reference.clone());
                            vec![reference]
                        }
                        None => vec![],
                    },
                    is_error: false,
                    step: event.source.kind.to_string(),
                })
                .await?;
        }

        Ok(())
    }
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
            let new_message = entity::messages::ActiveModel {
                id: ActiveValue::Set(Uuid::new_v4()),
                content: ActiveValue::Set(question),
                is_human: ActiveValue::Set(true),
                thread_id: ActiveValue::Set(thread.id),
                created_at: ActiveValue::default(),
            };
            new_message
                .insert(&connection)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        None => {
            if messages.len() > 1 {
                return Err(StatusCode::BAD_REQUEST);
            } else {
                user_question = thread.input.clone();
            }
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
        let markdown_writer = Arc::new(tokio::sync::Mutex::new(MarkdownWriter::default()));
        let references_arc = Arc::new(Mutex::new(vec![]));
        let thread_stream = ThreadStream::new(tx, references_arc.clone(), markdown_writer.clone());
        let result = run_agent(
            &project_path,
            &PathBuf::from(agent_ref),
            user_question,
            thread_stream,
            memory,
        )
        .await;
        match result {
            Ok(output_container) => {
                let references = try_unwrap_arc_mutex(references_arc)?;
                let markdown_writer = try_unwrap_arc_tokio_mutex(markdown_writer).await?;
                tracing::debug!("Agent output: {:?}", output_container);
                tracing::debug!("Agent references: {:?}", references);

                let answer_message = entity::messages::ActiveModel {
                    id: ActiveValue::Set(Uuid::new_v4()),
                    content: ActiveValue::Set(markdown_writer.finish().await?),
                    is_human: ActiveValue::Set(false),
                    thread_id: ActiveValue::Set(thread.id),
                    created_at: ActiveValue::default(),
                };
                answer_message.insert(&connection).await.map_err(|err| {
                    OxyError::DBError(format!("Failed to insert message:\n{}", err))
                })?;
                Result::<(), OxyError>::Ok(())
            }
            Err(err) => {
                tracing::error!("Error running agent: {}", err);
                let message = AnswerStream {
                    content: format!("Error running agent: {}", err),
                    references: vec![],
                    is_error: true,
                    step: "".to_string(),
                };
                tx_clone
                    .send(message)
                    .await
                    .map_err(|_| OxyError::RuntimeError("Failed to send message".to_string()))?;
                Result::<(), OxyError>::Ok(())
            }
        }
    });
    Ok(StreamBodyAs::json_nl(ReceiverStream::new(rx)))
}

#[derive(Deserialize)]
pub struct AskAgentRequest {
    pub question: String,
}

pub async fn ask_agent(
    Path(pathb64): Path<String>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    extract::Json(payload): extract::Json<AskAgentRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let decoded_path = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;

    let path = String::from_utf8(decoded_path).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;

    let project_path = find_project_path().map_err(|e| {
        tracing::error!("Failed to find project path: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let (tx, rx) = tokio::sync::mpsc::channel(100);
    let _ = tokio::spawn(async move {
        let tx_clone = tx.clone();
        let markdown_writer = Arc::new(tokio::sync::Mutex::new(MarkdownWriter::default()));
        let references_arc = Arc::new(Mutex::new(vec![]));
        let thread_stream = ThreadStream::new(tx, references_arc.clone(), markdown_writer.clone());
        let result = run_agent(
            &project_path,
            &PathBuf::from(path),
            payload.question,
            thread_stream,
            vec![],
        )
        .await;

        if let Err(err) = result {
            tracing::error!("Error running agent: {}", err);
            let message = AnswerStream {
                content: format!("Error running agent: {}", err),
                references: vec![],
                is_error: true,
                step: "".to_string(),
            };
            let _ = tx_clone.send(message).await;
        }
    });

    Ok(StreamBodyAs::json_nl(ReceiverStream::new(rx)))
}
