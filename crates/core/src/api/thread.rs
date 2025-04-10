use crate::db::client::establish_connection;
use crate::errors::OxyError;
use crate::execute::agent::AgentReference;
use crate::execute::types::{Event, EventKind, Output};
use crate::execute::writer::EventHandler;
use crate::service::agent::run_agent;
use crate::utils::find_project_path;
use async_stream::stream;
use axum::extract::{self, Path};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum_streams::StreamBodyAs;
use entity::prelude::Threads;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{ActiveModelTrait, ActiveValue, EntityTrait};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

#[derive(Serialize)]
pub struct ThreadItem {
    pub id: String,
    pub title: String,
    pub question: String,
    pub answer: String,
    pub agent: String,
    pub created_at: DateTimeWithTimeZone,
    pub references: Vec<AgentReference>,
}

#[derive(Deserialize)]
pub struct CreateThreadRequest {
    pub title: String,
    pub question: String,
    pub agent: String,
}

#[derive(Serialize)]
pub struct AnswerStream {
    pub content: String,
    pub references: Vec<AgentReference>,
    pub is_error: bool,
}

pub async fn get_threads() -> Result<extract::Json<Vec<ThreadItem>>, StatusCode> {
    let connection = establish_connection().await;
    let threads = Threads::find().all(&connection).await;
    let threads = threads.unwrap();
    let thread_items = threads
        .into_iter()
        .map(|t| ThreadItem {
            id: t.id.to_string(),
            title: t.title.clone(),
            question: t.question.clone(),
            answer: t.answer.clone(),
            agent: t.agent.clone(),
            created_at: t.created_at,
            references: serde_json::from_str(&t.references).unwrap(),
        })
        .collect();
    Ok(extract::Json(thread_items))
}

pub async fn get_thread(Path(id): Path<String>) -> Result<extract::Json<ThreadItem>, StatusCode> {
    let connection = establish_connection().await;
    let thread = Threads::find_by_id(Uuid::parse_str(&id).unwrap())
        .one(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let thread_item = ThreadItem {
        id: thread.id.to_string(),
        title: thread.title,
        question: thread.question,
        answer: thread.answer,
        agent: thread.agent,
        created_at: thread.created_at,
        references: serde_json::from_str(&thread.references).unwrap(),
    };
    Ok(extract::Json(thread_item))
}

pub async fn create_thread(
    extract::Json(thread_request): extract::Json<CreateThreadRequest>,
) -> Result<extract::Json<ThreadItem>, StatusCode> {
    let connection = establish_connection().await;
    let new_thread = entity::threads::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        created_at: ActiveValue::not_set(),
        title: ActiveValue::Set(thread_request.title),
        question: ActiveValue::Set(thread_request.question),
        answer: ActiveValue::Set("".to_string()),
        agent: ActiveValue::Set(thread_request.agent),
        references: ActiveValue::Set("[]".to_string()),
    };
    let thread = new_thread.insert(&connection).await;
    let thread = thread.unwrap();
    let thread_item = ThreadItem {
        id: thread.id.to_string(),
        title: thread.title,
        question: thread.question,
        answer: thread.answer,
        agent: thread.agent,
        created_at: thread.created_at,
        references: serde_json::from_str(&thread.references).unwrap(),
    };
    Ok(extract::Json(thread_item))
}

pub async fn delete_thread(Path(id): Path<String>) -> Result<StatusCode, StatusCode> {
    let connection = establish_connection().await;
    let thread = Threads::find_by_id(Uuid::parse_str(&id).unwrap())
        .one(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(thread) = thread {
        let active_thread: entity::threads::ActiveModel = thread.into();
        active_thread
            .delete(&connection)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok(StatusCode::OK)
}

pub async fn delete_all_threads() -> Result<StatusCode, StatusCode> {
    let connection = establish_connection().await;
    Threads::delete_many()
        .exec(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

struct ThreadStream {
    tx: Sender<AnswerStream>,
}

impl ThreadStream {
    fn new(tx: Sender<AnswerStream>) -> Self {
        ThreadStream { tx }
    }
}

#[async_trait::async_trait]
impl EventHandler for ThreadStream {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        if let EventKind::Updated { chunk } = event.kind {
            match chunk.delta {
                Output::Text(text) => {
                    let message = AnswerStream {
                        content: text,
                        references: vec![],
                        is_error: false,
                    };
                    self.tx.send(message).await?;
                }
                Output::Table(table) => {
                    let reference = table.into_reference();
                    let message = AnswerStream {
                        content: "".to_string(),
                        references: reference.map(|r| vec![r]).unwrap_or_default(),
                        is_error: false,
                    };
                    self.tx.send(message).await?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

pub async fn ask_thread(Path(id): Path<String>) -> impl IntoResponse {
    let connection = establish_connection().await;
    let thread = match Uuid::parse_str(&id) {
        Ok(uuid) => match Threads::find_by_id(uuid).one(&connection).await {
            Ok(Some(thread)) => thread,
            Ok(None) => {
                return StreamBodyAs::json_nl(stream! {
                    yield AnswerStream {
                        content: format!("Thread with ID {} not found", id),
                        references: vec![],
                        is_error: true,
                    };
                });
            }
            Err(e) => {
                return StreamBodyAs::json_nl(stream! {
                    yield AnswerStream {
                        content: format!("Database error: {}", e),
                        references: vec![],
                        is_error: true,
                    };
                });
            }
        },
        Err(_) => {
            return StreamBodyAs::json_nl(stream! {
                yield AnswerStream {
                    content: format!("Invalid UUID format: {}", id),
                    references: vec![],
                    is_error: true,
                };
            });
        }
    };

    if !thread.answer.is_empty() {
        return StreamBodyAs::json_nl(stream! {
            yield AnswerStream {
                content: thread.answer,
                references: serde_json::from_str(&thread.references).unwrap_or_default(),
                is_error: false,
            };
        });
    }

    let project_path = match find_project_path() {
        Ok(path) => path,
        Err(e) => {
            return StreamBodyAs::json_nl(stream! {
                yield AnswerStream {
                    content: format!("Failed to find project path: {}", e),
                    references: vec![],
                    is_error: true,
                };
            });
        }
    };

    let agent_ref = thread.agent.to_string();
    let prompt = thread.question.to_string();
    let (tx, rx) = tokio::sync::mpsc::channel(100);
    let _ = tokio::spawn(async move {
        let (output, references) = {
            let thread_stream = ThreadStream::new(tx);
            run_agent(
                &project_path,
                &PathBuf::from(agent_ref),
                prompt,
                thread_stream,
            )
            .await?
        };
        log::debug!("Agent output: {}", output);
        log::debug!("Agent references: {:?}", references);
        let mut thread_model: entity::threads::ActiveModel = thread.into();
        thread_model.answer = ActiveValue::Set(output);
        thread_model.references =
            ActiveValue::Set(serde_json::to_string(&references).map_err(|err| {
                OxyError::SerializerError(format!("Failed to serialize references:\n{}", err))
            })?);
        thread_model
            .update(&connection)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to update thread:\n{}", err)))
    });
    StreamBodyAs::json_nl(ReceiverStream::new(rx))
}
