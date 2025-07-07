use async_trait::async_trait;
use axum::{http::StatusCode, response::IntoResponse, response::sse::Sse};
use entity::prelude::{Messages, Threads};
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, Order, QueryFilter, QueryOrder,
    QuerySelect,
};
use std::{path::PathBuf, sync::Arc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    db::client::establish_connection,
    errors::OxyError,
    execute::types::Usage,
    service::{
        agent::Message,
        formatters::streaming_message_persister::StreamingMessagePersister,
        task_manager::TASK_MANAGER,
        types::{AnswerContent, AnswerStream},
    },
    utils::{create_sse_stream_with_cancellation, find_project_path},
};

pub trait ChatExecutionRequest {
    fn get_question(&self) -> Option<String>;
}

#[derive(Clone)]
pub struct ChatExecutionContext {
    pub thread: entity::threads::Model,
    pub user_question: String,
    pub memory: Vec<Message>,
    pub project_path: PathBuf,
    pub streaming_persister: Arc<StreamingMessagePersister>,
    pub cancellation_tokens: CancellationTokens,
}

impl ChatExecutionContext {
    pub fn new(
        thread: entity::threads::Model,
        user_question: String,
        memory: Vec<Message>,
        project_path: PathBuf,
        streaming_persister: Arc<StreamingMessagePersister>,
        cancellation_tokens: CancellationTokens,
    ) -> Self {
        Self {
            thread,
            user_question,
            memory,
            project_path,
            streaming_persister,
            cancellation_tokens,
        }
    }
}

#[derive(Clone)]
pub struct CancellationTokens {
    pub task_token: CancellationToken,
    pub stream_token: CancellationToken,
}

impl CancellationTokens {
    pub fn new() -> Self {
        Self {
            task_token: CancellationToken::new(),
            stream_token: CancellationToken::new(),
        }
    }
}

#[async_trait]
pub trait ChatHandler: Send + Sync {
    async fn execute(
        &self,
        context: ChatExecutionContext,
        tx: tokio::sync::mpsc::Sender<AnswerStream>,
    ) -> Result<(String, Usage), OxyError>;
}

pub struct ChatService {
    connection: sea_orm::DatabaseConnection,
}

impl ChatService {
    pub async fn new() -> Result<Self, StatusCode> {
        let connection = establish_connection().await;
        Ok(Self { connection })
    }

    pub async fn execute_request<T: ChatExecutionRequest, E: ChatHandler + 'static>(
        self,
        id: String,
        payload: T,
        executor: E,
        user_id: Uuid,
    ) -> Result<impl IntoResponse, StatusCode> {
        let thread_id = self.parse_thread_id(&id)?;
        let thread = self.validate_and_lock_thread(thread_id, user_id).await?;

        let user_question = self.handle_user_question(&payload, &thread).await?;
        let memory = self.build_conversation_memory(thread.id).await?;
        let project_path = find_project_path().map_err(|e| {
            tracing::error!("Failed to find project path: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let streaming_persister = Arc::new(
            StreamingMessagePersister::new(self.connection.clone(), thread.id, "".to_owned())
                .await
                .map_err(|err| {
                    OxyError::DBError(format!(
                        "Failed to create streaming message handler: {}",
                        err
                    ))
                })?,
        );

        let cancellation_tokens = CancellationTokens::new();
        let stream_token = cancellation_tokens.stream_token.clone();

        let execution_context = ChatExecutionContext::new(
            thread.clone(),
            user_question,
            memory,
            project_path,
            streaming_persister,
            cancellation_tokens,
        );

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        let _task_handle = self.spawn_execution_task(execution_context, tx, executor);

        Ok(Sse::new(create_sse_stream_with_cancellation(
            rx,
            stream_token,
        )))
    }

    fn parse_thread_id(&self, id: &str) -> Result<Uuid, StatusCode> {
        Uuid::parse_str(id).map_err(|e| {
            tracing::warn!("Invalid thread ID format: {}", e);
            StatusCode::BAD_REQUEST
        })
    }

    async fn validate_and_lock_thread(
        &self,
        thread_id: Uuid,
        user_id: Uuid,
    ) -> Result<entity::threads::Model, StatusCode> {
        let query =
            Threads::find_by_id(thread_id).filter(entity::threads::Column::UserId.eq(user_id));

        let thread = query
            .one(&self.connection)
            .await
            .map_err(|e| {
                tracing::error!("Database error finding thread: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or(StatusCode::NOT_FOUND)?;

        if thread.is_processing {
            return Err(StatusCode::CONFLICT);
        }

        let mut thread_model: entity::threads::ActiveModel = thread.clone().into();
        thread_model.is_processing = ActiveValue::Set(true);

        // Fix: Ensure we're returning the original thread, not the update result
        thread_model.update(&self.connection).await.map_err(|e| {
            tracing::error!("Failed to lock thread: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        Ok(thread)
    }

    async fn handle_user_question<T: ChatExecutionRequest>(
        &self,
        payload: &T,
        thread: &entity::threads::Model,
    ) -> Result<String, StatusCode> {
        let user_question = match payload.get_question() {
            Some(question) => {
                let new_message = entity::messages::ActiveModel {
                    id: ActiveValue::Set(Uuid::new_v4()),
                    content: ActiveValue::Set(question.clone()),
                    is_human: ActiveValue::Set(true),
                    thread_id: ActiveValue::Set(thread.id),
                    created_at: ActiveValue::default(),
                    ..Default::default()
                };
                new_message.insert(&self.connection).await.map_err(|e| {
                    tracing::error!("Failed to insert user message: {}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
                question
            }
            None => {
                let messages = Messages::find()
                    .filter(entity::messages::Column::ThreadId.eq(thread.id))
                    .limit(1)
                    .all(&self.connection)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

                if messages.len() > 1 {
                    return Err(StatusCode::BAD_REQUEST);
                }

                thread.input.clone()
            }
        };

        Ok(user_question)
    }

    async fn build_conversation_memory(&self, thread_id: Uuid) -> Result<Vec<Message>, StatusCode> {
        let mut messages = Messages::find()
            .filter(entity::messages::Column::ThreadId.eq(thread_id))
            .order_by(entity::messages::Column::CreatedAt, Order::Desc)
            .limit(10)
            .all(&self.connection)
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch conversation history: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        let memory = messages
            .into_iter()
            .map(|message| Message {
                content: message.content,
                is_human: message.is_human,
                created_at: message.created_at,
            })
            .collect();

        Ok(memory)
    }

    fn spawn_execution_task<E: ChatHandler + Send + 'static>(
        &self,
        context: ChatExecutionContext,
        tx: tokio::sync::mpsc::Sender<AnswerStream>,
        executor: E,
    ) {
        let connection = self.connection.clone();
        let thread_id = context.thread.id;
        let streaming_persister = context.streaming_persister.clone();
        let stream_token = context.cancellation_tokens.stream_token.clone();
        let task_token = context.cancellation_tokens.task_token.clone();

        let task_handle = tokio::spawn(async move {
            let result = tokio::select! {
                execution_result = executor.execute(context.clone(), tx.clone()) => {
                    execution_result
                }
                _ = context.cancellation_tokens.task_token.cancelled() => {
                    tracing::info!("Execution cancelled for thread: {}", context.thread.id);

                    let cancellation_message = AnswerStream {
                        content: AnswerContent::Text {
                            content: "🔴 Operation cancelled".to_string(),
                        },
                        references: vec![],
                        is_error: false,
                        step: "system".to_string(),
                    };

                    let _ = tx.send(cancellation_message).await;
                    stream_token.cancel();
                    let _ = streaming_persister.cancel("🔴 Operation cancelled").await;
                     return;
                }
            };

            match result {
                Ok(res) => {
                    let (output, usage) = res;
                    if let Err(e) = Self::handle_success(
                        output,
                        usage,
                        &context.thread,
                        streaming_persister.get_message_id(),
                        &connection,
                    )
                    .await
                    {
                        tracing::error!("Error handling success: {}", e);
                    }
                }
                Err(err) => {
                    if let Err(e) = Self::handle_error(
                        err,
                        streaming_persister.get_message_id(),
                        &context.thread,
                        tx.clone(),
                        &connection,
                    )
                    .await
                    {
                        tracing::error!("Error handling error: {}", e);
                    }
                }
            }

            TASK_MANAGER.remove_task(thread_id).await;
            Self::unlock_thread(&context.thread, &connection).await;
        });

        tokio::spawn(async move {
            TASK_MANAGER
                .register_task(thread_id, task_handle, task_token.clone())
                .await;
        });
    }

    async fn handle_success(
        output: String,
        usage: Usage,
        thread: &entity::threads::Model,
        message_id: Uuid,
        connection: &sea_orm::DatabaseConnection,
    ) -> Result<(), OxyError> {
        let answer_message = entity::messages::ActiveModel {
            id: ActiveValue::Set(message_id),
            content: ActiveValue::Set(output),
            is_human: ActiveValue::Set(false),
            thread_id: ActiveValue::Set(thread.id),
            created_at: ActiveValue::default(),
            input_tokens: ActiveValue::Set(usage.input_tokens),
            output_tokens: ActiveValue::Set(usage.output_tokens),
        };
        answer_message.update(connection).await.map_err(|err| {
            OxyError::DBError(format!("Failed to insert agent message:\n{}", err))
        })?;
        Ok(())
    }

    async fn handle_error(
        error: OxyError,
        message_id: Uuid,
        thread: &entity::threads::Model,
        tx: tokio::sync::mpsc::Sender<AnswerStream>,
        connection: &sea_orm::DatabaseConnection,
    ) -> Result<(), OxyError> {
        tracing::error!("Error running agent: {}", error);
        let error_message = format!("🔴 Error: {}", error);
        let error_message_model = entity::messages::ActiveModel {
            id: ActiveValue::Set(message_id),
            content: ActiveValue::Set(error_message.clone()),
            is_human: ActiveValue::Set(false),
            thread_id: ActiveValue::Set(thread.id),
            created_at: ActiveValue::default(),
            input_tokens: ActiveValue::Set(0),
            output_tokens: ActiveValue::Set(0),
        };

        error_message_model
            .insert(connection)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to insert error message: {}", err)))?;

        let error_event = AnswerStream {
            content: AnswerContent::Error {
                message: error_message,
            },
            references: vec![],
            is_error: true,
            step: "".to_string(),
        };

        tx.send(error_event)
            .await
            .map_err(|_| OxyError::RuntimeError("Failed to send error message".to_string()))?;

        Ok(())
    }

    async fn unlock_thread(
        thread: &entity::threads::Model,
        connection: &sea_orm::DatabaseConnection,
    ) {
        let mut thread_model: entity::threads::ActiveModel = thread.clone().into();
        thread_model.is_processing = ActiveValue::Set(false);

        if let Err(e) = thread_model.update(connection).await {
            tracing::error!("Failed to unlock thread {}: {}", thread.id, e);
        }
    }
}
