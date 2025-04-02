use crate::config::ConfigBuilder;
use crate::db::client::establish_connection;
use crate::execute::agent::AgentReference;
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
use std::time::Duration;
use uuid::Uuid;

use crate::{config::model::FileFormat, execute::agent::run_agent};

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

pub async fn ask_thread(Path(id): Path<String>) -> impl IntoResponse {
    let connection = establish_connection().await;
    let thread = match Uuid::parse_str(&id) {
        Ok(uuid) => match Threads::find_by_id(uuid).one(&connection).await {
            Ok(Some(thread)) => thread,
            Ok(None) => {
                return StreamBodyAs::json_nl(stream! {
                    yield AnswerStream {
                        content: format!("Thread with ID {} not found", id),
                        is_error: true,
                    };
                });
            }
            Err(e) => {
                return StreamBodyAs::json_nl(stream! {
                    yield AnswerStream {
                        content: format!("Database error: {}", e),
                        is_error: true,
                    };
                });
            }
        },
        Err(_) => {
            return StreamBodyAs::json_nl(stream! {
                yield AnswerStream {
                    content: format!("Invalid UUID format: {}", id),
                    is_error: true,
                };
            });
        }
    };

    if !thread.answer.is_empty() {
        return StreamBodyAs::json_nl(stream! {
            yield AnswerStream {
                content: thread.answer,
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
                    is_error: true,
                };
            });
        }
    };

    let config_builder = match ConfigBuilder::new().with_project_path(&project_path) {
        Ok(config_builder) => config_builder,
        Err(e) => {
            return StreamBodyAs::json_nl(stream! {
                yield AnswerStream {
                    content: format!("Failed to build config: {}", e),
                    is_error: true,
                };
            });
        }
    };

    let config = match config_builder.build().await {
        Ok(config) => std::sync::Arc::new(config),
        Err(e) => {
            return StreamBodyAs::json_nl(stream! {
                yield AnswerStream {
                    content: format!("Failed to build config: {}", e),
                    is_error: true,
                };
            });
        }
    };

    let result = match run_agent(
        &project_path.join(thread.agent.clone()),
        &FileFormat::Markdown,
        Some(thread.question.clone()),
        config,
        None,
    )
    .await
    {
        Ok(output) => output,
        Err(e) => {
            return StreamBodyAs::json_nl(stream! {
                yield AnswerStream {
                    content: format!("Error running agent: {}", e),
                    is_error: true,
                };
            });
        }
    };

    let mut thread_model: entity::threads::ActiveModel = thread.into();
    thread_model.answer = ActiveValue::Set(result.output.to_string());
    thread_model.references = ActiveValue::Set(serde_json::to_string(&result.references).unwrap());
    if let Err(e) = thread_model.update(&connection).await {
        return StreamBodyAs::json_nl(stream! {
            yield AnswerStream {
                content: format!("Failed to update thread: {}", e),
                is_error: true,
            };
        });
    }

    let s = stream! {
        for c in result.output.to_string().chars() {
            tokio::time::sleep(Duration::from_millis(5)).await;
            yield AnswerStream {
                content: c.to_string(),
                is_error: false,
            };
        }
    };

    StreamBodyAs::json_nl(s)
}
