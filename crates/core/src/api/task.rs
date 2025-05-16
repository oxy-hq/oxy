use std::sync::{Arc, Mutex};

use crate::{
    config::ConfigBuilder,
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
use entity::prelude::Tasks;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{ActiveModelTrait, ActiveValue, EntityTrait};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

#[derive(Serialize)]
pub struct TaskItem {
    pub id: String,
    pub title: String,
    pub question: String,
    pub answer: String,
    pub created_at: DateTimeWithTimeZone,
    pub file_path: String,
}

#[derive(Deserialize)]
pub struct CreateThreadRequest {
    pub title: String,
    pub question: String,
}

#[derive(Serialize)]
pub struct AnswerStream {
    pub content: String,
    pub file_path: String,
    pub is_error: bool,
    pub step: String,
}

pub async fn get_tasks() -> Result<extract::Json<Vec<TaskItem>>, StatusCode> {
    let connection = establish_connection().await;
    let tasks = Tasks::find().all(&connection).await.unwrap();
    let task_items = tasks
        .into_iter()
        .map(|t| TaskItem {
            id: t.id.to_string(),
            title: t.title.clone(),
            question: t.question.clone(),
            answer: t.answer.clone(),
            created_at: t.created_at,
            file_path: t.file_path.clone(),
        })
        .collect();
    Ok(extract::Json(task_items))
}

pub async fn get_task(Path(id): Path<String>) -> Result<extract::Json<TaskItem>, StatusCode> {
    let connection = establish_connection().await;
    let task = Tasks::find_by_id(Uuid::parse_str(&id).unwrap())
        .one(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let task_item = TaskItem {
        id: task.id.to_string(),
        title: task.title,
        question: task.question,
        answer: task.answer,
        created_at: task.created_at,
        file_path: task.file_path,
    };
    Ok(extract::Json(task_item))
}

pub async fn create_task(
    extract::Json(thread_request): extract::Json<CreateThreadRequest>,
) -> Result<extract::Json<TaskItem>, StatusCode> {
    let connection = establish_connection().await;
    let new_task = entity::tasks::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        created_at: ActiveValue::not_set(),
        title: ActiveValue::Set(thread_request.title),
        question: ActiveValue::Set(thread_request.question),
        answer: ActiveValue::Set("".to_string()),
        file_path: ActiveValue::Set("".to_string()),
    };
    let task = new_task.insert(&connection).await.map_err(|err| {
        tracing::error!("Failed to create task: {}", err);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let task_item = TaskItem {
        id: task.id.to_string(),
        title: task.title,
        question: task.question,
        answer: task.answer,
        created_at: task.created_at,
        file_path: task.file_path,
    };
    Ok(extract::Json(task_item))
}

pub async fn delete_task(Path(id): Path<String>) -> Result<StatusCode, StatusCode> {
    let connection = establish_connection().await;
    let thread = Tasks::find_by_id(Uuid::parse_str(&id).unwrap())
        .one(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(thread) = thread {
        let active_task: entity::tasks::ActiveModel = thread.into();
        active_task
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

pub async fn delete_all_tasks() -> Result<StatusCode, StatusCode> {
    let connection = establish_connection().await;
    Tasks::delete_many()
        .exec(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    {
        use crate::db::client::get_charts_dir;
        remove_all_files_in_dir(get_charts_dir());
    }

    Ok(StatusCode::OK)
}

struct TaskStream {
    references: Arc<Mutex<Vec<ReferenceKind>>>,
    tx: Sender<AnswerStream>,
}

impl TaskStream {
    fn new(tx: Sender<AnswerStream>) -> Self {
        TaskStream {
            tx,
            references: Arc::new(Mutex::new(vec![])),
        }
    }
}

#[async_trait::async_trait]
impl EventHandler for TaskStream {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        if let EventKind::Updated { chunk } = &event.kind {
            match &chunk.delta {
                Output::Prompt(_) => {
                    let message = AnswerStream {
                        content: "".to_string(),
                        is_error: false,
                        step: event.source.kind.to_string(),
                        file_path: "".to_string(),
                    };
                    __self.tx.send(message).await?;
                }
                Output::Text(text) => {
                    let message = AnswerStream {
                        content: text.to_owned(),
                        is_error: false,
                        step: event.source.kind.to_string(),
                        file_path: "".to_string(),
                    };
                    __self.tx.send(message).await?;
                }
                Output::Table(table) => {
                    let reference = table.clone().into_reference();
                    match reference {
                        Some(r) => {
                            __self.references.lock().unwrap().push(r);
                            let message = AnswerStream {
                                content: "".to_string(),
                                is_error: false,
                                step: event.source.kind.to_string(),
                                file_path: "".to_string(),
                            };
                            __self.tx.send(message).await?;
                        }
                        None => {}
                    }
                }
                Output::Bool(_) => {}
                Output::SQL(sql) => {}
                Output::Documents(items) => {}
            }
        }
        if let EventKind::DataAppCreated { data_app } = &event.kind {
            let message = AnswerStream {
                content: "".to_string(),
                is_error: false,
                step: event.source.kind.to_string(),
                file_path: data_app.file_path.to_string_lossy().to_string(),
            };
            self.tx.send(message).await?;
        }
        Ok(())
    }
}

pub async fn ask_task(Path(id): Path<String>) -> impl IntoResponse {
    let connection = establish_connection().await;
    let task = match Uuid::parse_str(&id) {
        Ok(uuid) => match Tasks::find_by_id(uuid).one(&connection).await {
            Ok(Some(thread)) => thread,
            Ok(None) => {
                return StreamBodyAs::json_nl(stream! {
                    yield AnswerStream {
                        content: format!("Thread with ID {} not found", id),
                        is_error: true,
                        step: "".to_string(),
                        file_path: "".to_string()
                    };
                });
            }
            Err(e) => {
                return StreamBodyAs::json_nl(stream! {
                    yield AnswerStream {
                        content: format!("Database error: {}", e),
                        is_error: true,
                        step: "".to_string(),
                        file_path: "".to_string()
                    };
                });
            }
        },
        Err(_) => {
            return StreamBodyAs::json_nl(stream! {
                yield AnswerStream {
                    content: format!("Invalid UUID format: {}", id),
                    is_error: true,
                    step: "".to_string(),
                    file_path: "".to_string()
                };
            });
        }
    };

    if !task.answer.is_empty() {
        return StreamBodyAs::json_nl(stream! {
            yield AnswerStream {
                content: task.answer,
                is_error: false,
                step: "".to_string(),
                file_path: task.file_path,
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
                    step: "".to_string(),
                    file_path: "".to_string()
                };
            });
        }
    };

    let config = ConfigBuilder::new()
        .with_project_path(&project_path)
        .unwrap()
        .build()
        .await
        .unwrap();

    let agent_ref = config.get_builder_agent_path().await.unwrap();
    let prompt = task.question.to_string();
    let (tx, rx) = tokio::sync::mpsc::channel(100);
    let _ = tokio::spawn(async move {
        let tx_clone = tx.clone();
        let thread_stream = TaskStream::new(tx);
        let references = thread_stream.references.clone();
        let agent_result = { run_agent(&project_path, &agent_ref, prompt, thread_stream).await };

        match agent_result {
            Ok(output_container) => {
                let references = Arc::try_unwrap(references)
                    .map_err(|_| {
                        OxyError::RuntimeError("Failed to unwrap agent references".to_string())
                    })?
                    .into_inner()
                    .map_err(|_| {
                        OxyError::RuntimeError("Failed to lock agent references".to_string())
                    })?;
                tracing::debug!("Agent output: {:?}", output_container);
                tracing::debug!("Agent references: {:?}", references);
                let mut task_model: entity::tasks::ActiveModel = task.into();
                for r in references {
                    if let ReferenceKind::DataApp(data_app) = r {
                        let file_path = data_app.file_path.to_string_lossy().to_string();
                        task_model.file_path = ActiveValue::Set(file_path.clone());
                    }
                }
                task_model.answer = ActiveValue::Set(output_container.to_string());
                task_model.update(&connection).await.map_err(|err| {
                    OxyError::DBError(format!("Failed to update thread:\n{}", err))
                })?;
                Result::<(), OxyError>::Ok(())
            }
            Err(e) => {
                tracing::error!("Error running agent: {}", e);
                tx_clone
                    .send(AnswerStream {
                        content: format!("Error running agent: {}", e),
                        is_error: true,
                        step: "".to_string(),
                        file_path: "".to_string(),
                    })
                    .await?;
                Result::<(), OxyError>::Ok(())
            }
        }
    });
    StreamBodyAs::json_nl(ReceiverStream::new(rx))
}
