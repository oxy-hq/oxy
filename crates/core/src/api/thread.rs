use crate::{
    db::client::establish_connection,
    errors::OxyError,
    execute::{
        types::{Event, ReferenceKind},
        writer::{EventHandler, MarkdownWriter, OutputWriter},
    },
    service::agent::run_agent,
    utils::{find_project_path, try_unwrap_arc_mutex, try_unwrap_arc_tokio_mutex},
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

pub async fn get_threads() -> Result<extract::Json<Vec<ThreadItem>>, StatusCode> {
    let connection = establish_connection().await;
    let threads = Threads::find().all(&connection).await;
    let threads = threads.unwrap();
    if threads.is_empty() {
        return Ok(extract::Json(vec![]));
    }
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
    extract::Json(thread_request): extract::Json<CreateThreadRequest>,
) -> Result<extract::Json<ThreadItem>, StatusCode> {
    let connection = establish_connection().await;
    let new_thread = entity::threads::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        created_at: ActiveValue::not_set(),
        title: ActiveValue::Set(thread_request.title),
        input: ActiveValue::Set(thread_request.input),
        output: ActiveValue::Set("".to_string()),
        source_type: ActiveValue::Set(thread_request.source_type),
        source: ActiveValue::Set(thread_request.source),
        references: ActiveValue::Set("[]".to_string()),
    };
    let thread = new_thread.insert(&connection).await;
    let thread = thread.unwrap();
    let thread_item = ThreadItem {
        id: thread.id.to_string(),
        title: thread.title,
        input: thread.input,
        output: thread.output,
        source_type: thread.source_type,
        source: thread.source,
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

pub async fn ask_thread(Path(id): Path<String>) -> Result<impl IntoResponse, StatusCode> {
    let connection = establish_connection().await;
    let thread_id = Uuid::parse_str(&id).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let thread = Threads::find_by_id(thread_id)
        .one(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let thread = thread.ok_or(StatusCode::NOT_FOUND)?;

    if !thread.output.is_empty() {
        return Ok(StreamBodyAs::json_nl(stream! {
            yield AnswerStream {
                content: thread.output,
                references: serde_json::from_str(&thread.references).unwrap_or_default(),
                is_error: false,
                step: "".to_string(),
            };
        }));
    }

    let project_path = find_project_path().map_err(|e| {
        tracing::info!("Failed to find project path: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let agent_ref = thread.source.to_string();
    let prompt = thread.input.to_string();
    let (tx, rx) = tokio::sync::mpsc::channel(100);
    let _ = tokio::spawn(async move {
        let tx_clone = tx.clone();
        let markdown_writer = Arc::new(tokio::sync::Mutex::new(MarkdownWriter::default()));
        let references_arc = Arc::new(Mutex::new(vec![]));
        let thread_stream = ThreadStream::new(tx, references_arc.clone(), markdown_writer.clone());
        let result = run_agent(
            &project_path,
            &PathBuf::from(agent_ref),
            prompt,
            thread_stream,
        )
        .await;
        match result {
            Ok(output_container) => {
                let references = try_unwrap_arc_mutex(references_arc)?;
                let markdown_writer = try_unwrap_arc_tokio_mutex(markdown_writer).await?;
                tracing::debug!("Agent output: {:?}", output_container);
                tracing::debug!("Agent references: {:?}", references);
                let mut thread_model: entity::threads::ActiveModel = thread.into();
                thread_model.output = ActiveValue::Set(markdown_writer.finish().await?);
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
    Ok(StreamBodyAs::json_nl(ReceiverStream::new(rx)))
}

#[derive(Deserialize)]
pub struct AskAgentRequest {
    pub question: String,
}

pub async fn ask_agent(
    Path(pathb64): Path<String>,
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
