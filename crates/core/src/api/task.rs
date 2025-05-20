use std::sync::{Arc, Mutex};

use crate::{
    config::ConfigBuilder,
    db::client::establish_connection,
    errors::OxyError,
    execute::{
        types::{DataAppReference, Event, EventKind, Output, ReferenceKind},
        writer::EventHandler,
    },
    service::agent::run_agent,
    utils::find_project_path,
};
use async_stream::stream;
use axum::{extract::Path, http::StatusCode, response::IntoResponse};
use axum_streams::StreamBodyAs;
use entity::prelude::Threads;
use sea_orm::{ActiveModelTrait, ActiveValue, EntityTrait};
use serde::Serialize;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

#[derive(Serialize)]
pub struct AnswerStream {
    pub content: String,
    pub file_path: String,
    pub is_error: bool,
    pub step: String,
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
                    if let Some(r) = reference {
                        __self.references.lock().unwrap().push(r);
                        let message = AnswerStream {
                            content: "".to_string(),
                            is_error: false,
                            step: event.source.kind.to_string(),
                            file_path: "".to_string(),
                        };
                        __self.tx.send(message).await?;
                    }
                }
                Output::Bool(_) => {}
                Output::SQL(sql) => {}
                Output::Documents(items) => {}
            }
        }
        if let EventKind::DataAppCreated { data_app } = &event.kind {
            __self
                .references
                .lock()
                .unwrap()
                .push(ReferenceKind::DataApp(DataAppReference {
                    file_path: data_app.file_path.clone(),
                }));
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

pub async fn ask_task(Path(id): Path<String>) -> Result<impl IntoResponse, StatusCode> {
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
                file_path: thread.source,
                is_error: false,
                step: "".to_string(),
            };
        }));
    }

    let project_path = find_project_path().map_err(|e| {
        tracing::info!("Failed to find project path: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let config = ConfigBuilder::new()
        .with_project_path(&project_path)
        .unwrap()
        .build()
        .await
        .unwrap();

    let agent_ref = config.get_builder_agent_path().await.unwrap();
    let prompt = thread.input.to_string();
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
                let mut thread_model: entity::threads::ActiveModel = thread.into();
                for r in references {
                    if let ReferenceKind::DataApp(data_app) = r {
                        let file_path = data_app.file_path.to_string_lossy().to_string();
                        thread_model.source = ActiveValue::Set(file_path.clone());
                    }
                }
                thread_model.output = ActiveValue::Set(output_container.to_string());
                thread_model.update(&connection).await.map_err(|err| {
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
    Ok(StreamBodyAs::json_nl(ReceiverStream::new(rx)))
}
