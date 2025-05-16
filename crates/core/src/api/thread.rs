use crate::{
    config::constants::WORKFLOW_SOURCE,
    db::client::establish_connection,
    errors::OxyError,
    execute::{
        types::{Event, EventKind, Output, ReferenceKind},
        writer::EventHandler,
    },
    service::agent::run_agent,
    utils::find_project_path,
};
use async_stream::stream;
use axum::{
    extract::{self, Path},
    http::StatusCode,
    response::IntoResponse,
};
use axum_streams::StreamBodyAs;
use base64::{Engine, prelude::BASE64_STANDARD};
use entity::prelude::Threads;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{ActiveModelTrait, ActiveValue, EntityTrait};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
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
    pub question: String,
    pub answer: String,
    pub agent: String,
    pub created_at: DateTimeWithTimeZone,
    pub references: Vec<ReferenceKind>,
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
    pub references: Vec<ReferenceKind>,
    pub is_error: bool,
    pub step: String,
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
            references: serde_json::from_str(&t.references).unwrap_or_default(),
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
        references: serde_json::from_str(&thread.references).unwrap_or_default(),
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

pub async fn delete_all_threads() -> Result<StatusCode, StatusCode> {
    let connection = establish_connection().await;
    Threads::delete_many()
        .exec(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    {
        use crate::db::client::get_charts_dir;
        remove_all_files_in_dir(get_charts_dir());
    }

    Ok(StatusCode::OK)
}

struct ThreadStream {
    references: Arc<Mutex<Vec<ReferenceKind>>>,
    tx: Sender<AnswerStream>,
    task_queue: VecDeque<String>,
}

impl ThreadStream {
    fn new(tx: Sender<AnswerStream>) -> Self {
        ThreadStream {
            tx,
            references: Arc::new(Mutex::new(vec![])),
            task_queue: VecDeque::new(),
        }
    }
}

#[async_trait::async_trait]
impl EventHandler for ThreadStream {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        if let EventKind::Started { name } = &event.kind {
            if event.source.kind.as_str() != WORKFLOW_SOURCE {
                self.task_queue.push_back(name.clone());
                let message = AnswerStream {
                    content: format!("<details open>\n<summary>{}</summary>\n\n", name),
                    references: vec![],
                    is_error: false,
                    step: "".to_string(),
                };
                self.tx.send(message).await?;
            }
        }

        if let EventKind::Finished { .. } = &event.kind {
            if event.source.kind.as_str() != WORKFLOW_SOURCE {
                if let Some(_) = self.task_queue.pop_back() {
                    let message = AnswerStream {
                        content: "\n\n</details>\n".to_string(),
                        references: vec![],
                        is_error: false,
                        step: "".to_string(),
                    };
                    self.tx.send(message).await?;
                }
            }
        }

        if let EventKind::Updated { chunk } = event.kind {
            match chunk.delta {
                Output::Prompt(_) => {
                    let message = AnswerStream {
                        content: "".to_string(),
                        references: vec![],
                        is_error: false,
                        step: event.source.kind.to_string(),
                    };
                    __self.tx.send(message).await?;
                }
                Output::Text(text) => {
                    let message = AnswerStream {
                        content: text,
                        references: vec![],
                        is_error: false,
                        step: event.source.kind.to_string(),
                    };
                    __self.tx.send(message).await?;
                }
                Output::Table(table) => {
                    let table_display = table.to_string();
                    let reference = table.into_reference();
                    match reference {
                        Some(r) => {
                            self.references.lock().unwrap().push(r.clone());
                            let message = AnswerStream {
                                content: table_display,
                                references: vec![r],
                                is_error: false,
                                step: event.source.kind.to_string(),
                            };
                            __self.tx.send(message).await?;
                        }
                        None => {}
                    }
                }
                Output::Bool(_) => {}
                Output::SQL(_) => {}
                Output::Documents(_) => {}
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
                        step: "".to_string(),
                    };
                });
            }
            Err(e) => {
                return StreamBodyAs::json_nl(stream! {
                    yield AnswerStream {
                        content: format!("Database error: {}", e),
                        references: vec![],
                        is_error: true,
                        step: "".to_string(),
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
                    step: "".to_string(),
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
                step: "".to_string(),
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
                    step: "".to_string(),
                };
            });
        }
    };

    let agent_ref = thread.agent.to_string();
    let prompt = thread.question.to_string();
    let (tx, rx) = tokio::sync::mpsc::channel(100);
    let _ = tokio::spawn(async move {
        let tx_clone = tx.clone();
        let thread_stream = ThreadStream::new(tx);
        let references_arc = thread_stream.references.clone();
        let result = {
            run_agent(
                &project_path,
                &PathBuf::from(agent_ref),
                prompt,
                thread_stream,
            )
            .await
        };
        match result {
            Ok(output_container) => {
                let references = Arc::try_unwrap(references_arc)
                    .map_err(|_| {
                        OxyError::RuntimeError("Failed to unwrap agent references".to_string())
                    })?
                    .into_inner()
                    .map_err(|err| {
                        OxyError::RuntimeError(format!("Failed to get agent references: {}", err))
                    })?;
                tracing::debug!("Agent output: {:?}", output_container);
                tracing::debug!("Agent references: {:?}", references);
                let mut thread_model: entity::threads::ActiveModel = thread.into();
                thread_model.answer = ActiveValue::Set(output_container.to_markdown()?);
                thread_model.references =
                    ActiveValue::Set(serde_json::to_string(&references).map_err(|err| {
                        OxyError::SerializerError(format!(
                            "Failed to serialize references:\n{}",
                            err
                        ))
                    })?);
                thread_model.update(&connection).await.map_err(|err| {
                    OxyError::DBError(format!("Failed to update thread:\n{}", err))
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
    StreamBodyAs::json_nl(ReceiverStream::new(rx))
}

#[derive(Deserialize)]
pub struct AskAgentRequest {
    pub question: String,
}

pub async fn ask_agent(
    Path(pathb64): Path<String>,
    extract::Json(payload): extract::Json<AskAgentRequest>,
) -> impl IntoResponse {
    let decoded_path: Vec<u8> = match BASE64_STANDARD.decode(pathb64) {
        Ok(path) => path,
        Err(e) => {
            return StreamBodyAs::json_nl(stream! {
                yield AnswerStream {
                    content: format!("Failed to decode path: {}", e),
                    references: vec![],
                    is_error: true,
                    step: "".to_string(),
                };
            });
        }
    };
    let path = match String::from_utf8(decoded_path) {
        Ok(path) => path,
        Err(e) => {
            return StreamBodyAs::json_nl(stream! {
                yield AnswerStream {
                    content: format!("Failed to decode path: {}", e),
                    references: vec![],
                    is_error: true,
                    step: "".to_string(),
                };
            });
        }
    };

    let project_path = match find_project_path() {
        Ok(path) => path,
        Err(e) => {
            return StreamBodyAs::json_nl(stream! {
                yield AnswerStream {
                    content: format!("Failed to find project path: {}", e),
                    references: vec![],
                    is_error: true,
                    step: "".to_string(),
                };
            });
        }
    };
    let (tx, rx) = tokio::sync::mpsc::channel(100);
    let _ = tokio::spawn(async move {
        let thread_stream = ThreadStream::new(tx);
        run_agent(
            &project_path,
            &PathBuf::from(path),
            payload.question,
            thread_stream,
        )
        .await
        .map_err(|err| OxyError::AgentError(format!("Failed to run agent:\n{}", err)))
    });
    StreamBodyAs::json_nl(ReceiverStream::new(rx))
}
