use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as TokioMutex;

use crate::{
    config::ConfigBuilder,
    db::client::establish_connection,
    errors::OxyError,
    execute::{
        types::{DataAppReference, Event, EventKind, Output, ReferenceKind, Usage},
        writer::EventHandler,
    },
    service::{
        agent::run_agent, formatters::streaming_message_persister::StreamingMessagePersister,
    },
    utils::{create_sse_stream, find_project_path},
};
use axum::{
    extract::{self, Path},
    http::StatusCode,
    response::IntoResponse,
    response::sse::Sse,
};
use entity::prelude::Threads;
use sea_orm::ColumnTrait;
use sea_orm::{ActiveModelTrait, ActiveValue, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;
use uuid::Uuid;

#[derive(Serialize)]
pub struct AnswerStream {
    pub content: String,
    pub file_path: String,
    pub is_error: bool,
    pub step: String,
    pub usage: Usage,
}

struct TaskStream {
    references: Arc<Mutex<Vec<ReferenceKind>>>,
    tx: Sender<AnswerStream>,
    usage: Arc<TokioMutex<Usage>>,
    streaming_message_persister: Arc<StreamingMessagePersister>,
}

impl TaskStream {
    fn new(
        tx: Sender<AnswerStream>,
        streaming_message_persister: Arc<StreamingMessagePersister>,
    ) -> Self {
        TaskStream {
            tx,
            references: Arc::new(Mutex::new(vec![])),
            usage: Arc::new(TokioMutex::new(Usage::new(0, 0))),
            streaming_message_persister,
        }
    }

    async fn update_usage(&self, usage: Usage) -> Result<(), OxyError> {
        let mut usage_lock = self.usage.lock().await;
        usage_lock.input_tokens += usage.input_tokens;
        usage_lock.output_tokens += usage.output_tokens;
        Ok(())
    }
}

#[async_trait::async_trait]
impl EventHandler for TaskStream {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        if let EventKind::Usage { usage } = &event.kind {
            self.update_usage(usage.clone()).await?;
        }

        let usage = self.usage.lock().await.clone();

        if let EventKind::Updated { chunk } = &event.kind {
            match &chunk.delta {
                Output::Prompt(_) => {
                    let message = AnswerStream {
                        content: "".to_string(),
                        is_error: false,
                        step: event.source.kind.to_string(),
                        file_path: "".to_string(),
                        usage: usage.clone(),
                    };
                    let _ = self.tx.send(message).await.map_err(|_| ());
                }
                Output::Text(text) => {
                    let content = text.clone();
                    let message = AnswerStream {
                        content: text.to_owned(),
                        is_error: false,
                        step: event.source.kind.to_string(),
                        file_path: "".to_string(),
                        usage: usage.clone(),
                    };
                    let _ = self.tx.send(message).await.map_err(|_| ());
                    self.streaming_message_persister
                        .append_content(&content)
                        .await?;
                }
                Output::Table(table) => {
                    let reference = table.clone().into_reference();
                    if let Some(r) = reference {
                        self.references.lock().unwrap().push(r);
                        let message = AnswerStream {
                            content: "".to_string(),
                            is_error: false,
                            step: event.source.kind.to_string(),
                            file_path: "".to_string(),
                            usage: usage.clone(),
                        };
                        let _ = self.tx.send(message).await.map_err(|_| ());
                    }
                }
                _ => {}
            }
        }
        if let EventKind::DataAppCreated { data_app } = &event.kind {
            self.references
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
                usage: usage.clone(),
            };
            let _ = self.tx.send(message).await.map_err(|_| ());
        }
        Ok(())
    }
}

#[derive(Deserialize)]
pub struct AskTaskRequest {
    pub question: Option<String>,
}

pub async fn ask_task(
    Path(id): Path<String>,
    extract::Json(payload): extract::Json<AskTaskRequest>,
) -> Result<impl IntoResponse, StatusCode> {
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

    if thread.is_processing {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut thread_model: entity::threads::ActiveModel = thread.clone().into();
    thread_model.is_processing = ActiveValue::Set(true);
    thread_model
        .update(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get existing messages for context
    let mut messages = entity::prelude::Messages::find()
        .filter(entity::messages::Column::ThreadId.eq(thread.id))
        .order_by(entity::messages::Column::CreatedAt, sea_orm::Order::Desc)
        .limit(10)
        .all(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    let user_question = match payload.question {
        Some(question) => {
            // Save the new user message
            let new_message = entity::messages::ActiveModel {
                id: ActiveValue::Set(Uuid::new_v4()),
                content: ActiveValue::Set(question.clone()),
                is_human: ActiveValue::Set(true),
                thread_id: ActiveValue::Set(thread.id),
                created_at: ActiveValue::default(),
                ..Default::default()
            };
            new_message
                .insert(&connection)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            question
        }
        None => {
            if messages.len() > 1 {
                return Err(StatusCode::BAD_REQUEST);
            } else {
                thread.input.clone()
            }
        }
    };

    let memory = messages
        .into_iter()
        .map(|message| crate::service::agent::Message {
            content: message.content,
            is_human: message.is_human,
            created_at: message.created_at,
        })
        .collect::<Vec<_>>();

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
    let (tx, rx) = tokio::sync::mpsc::channel(100);
    let _ = tokio::spawn(async move {
        let tx_clone = tx.clone();
        let streaming_message_handler = Arc::new(
            StreamingMessagePersister::new(connection.clone(), thread.id, "".to_owned())
                .await
                .map_err(|err| {
                    OxyError::DBError(format!("Failed to create streaming message handler: {err}"))
                })?,
        );
        let thread_stream = TaskStream::new(tx, streaming_message_handler.clone());
        let references = thread_stream.references.clone();
        let usage_arc = thread_stream.usage.clone();
        let agent_result = run_agent(
            &project_path,
            &agent_ref,
            user_question,
            thread_stream,
            memory,
        )
        .await;

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

                let usage = usage_arc.lock().await.clone();

                tracing::debug!("Agent output: {:?}", output_container);
                tracing::debug!("Agent references: {:?}", references);
                tracing::info!("Token usage: {:?}", usage);

                // Save the agent response to messages table
                let answer_message = entity::messages::ActiveModel {
                    id: ActiveValue::Set(streaming_message_handler.get_message_id()),
                    content: ActiveValue::Set(output_container.to_string()),
                    is_human: ActiveValue::Set(false),
                    thread_id: ActiveValue::Set(thread.id),
                    created_at: ActiveValue::default(),
                    input_tokens: ActiveValue::Set(usage.input_tokens),
                    output_tokens: ActiveValue::Set(usage.output_tokens),
                };
                answer_message.update(&connection).await.map_err(|err| {
                    OxyError::DBError(format!("Failed to insert agent message:\n{err}"))
                })?;

                let mut thread_model: entity::threads::ActiveModel = thread.into();
                for r in references {
                    if let ReferenceKind::DataApp(data_app) = r {
                        let file_path = data_app.file_path.to_string_lossy().to_string();
                        thread_model.source = ActiveValue::Set(file_path.clone());
                    }
                }
                thread_model.output = ActiveValue::Set(output_container.to_string());
                thread_model.is_processing = ActiveValue::Set(false);
                thread_model
                    .update(&connection)
                    .await
                    .map_err(|err| OxyError::DBError(format!("Failed to update thread:\n{err}")))?;
                Result::<(), OxyError>::Ok(())
            }
            Err(e) => {
                tracing::error!("Error running agent: {}", e);
                let msg = format!("🔴 Error: {e}");

                // Fallback: create error message normally
                let answer_message = entity::messages::ActiveModel {
                    id: ActiveValue::Set(streaming_message_handler.get_message_id()),
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
                tx_clone
                    .send(AnswerStream {
                        content: msg,
                        is_error: true,
                        step: "".to_string(),
                        file_path: "".to_string(),
                        usage: Usage::new(0, 0),
                    })
                    .await?;

                let mut thread_model: entity::threads::ActiveModel = thread.into();
                thread_model.is_processing = ActiveValue::Set(false);
                let _ = thread_model.update(&connection).await;
                Result::<(), OxyError>::Ok(())
            }
        }
    });

    let stream = create_sse_stream(rx);
    Ok(Sse::new(stream))
}
