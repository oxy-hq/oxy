use async_trait::async_trait;
use axum::{http::StatusCode, response::IntoResponse, response::sse::Sse};
use entity::{
    messages,
    prelude::{Messages, Threads},
};
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
    project::resolve_project_path,
    service::{
        agent::Message,
        formatters::{
            logs_persister::LogsPersister, streaming_message_persister::StreamingMessagePersister,
        },
        task_manager::TASK_MANAGER,
        types::{AnswerContent, AnswerStream},
    },
    utils::create_sse_stream_with_cancellation,
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
    pub logs_persister: Arc<LogsPersister>,
    pub cancellation_tokens: CancellationTokens,
}

impl ChatExecutionContext {
    pub fn new(
        thread: entity::threads::Model,
        user_question: String,
        memory: Vec<Message>,
        project_path: PathBuf,
        streaming_persister: Arc<StreamingMessagePersister>,
        logs_persister: Arc<LogsPersister>,
        cancellation_tokens: CancellationTokens,
    ) -> Self {
        Self {
            thread,
            user_question,
            memory,
            project_path,
            streaming_persister,
            logs_persister,
            cancellation_tokens,
        }
    }
}

#[derive(Clone)]
pub struct CancellationTokens {
    pub task_token: CancellationToken,
    pub stream_token: CancellationToken,
}

impl Default for CancellationTokens {
    fn default() -> Self {
        Self::new()
    }
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
        let connection = establish_connection().await.map_err(|e| {
            tracing::error!("Failed to establish database connection: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        Ok(Self { connection })
    }

    pub async fn execute_request<T: ChatExecutionRequest, E: ChatHandler + 'static>(
        self,
        id: String,
        payload: T,
        executor: E,
        user_id: Uuid,
    ) -> Result<impl IntoResponse, StatusCode> {
        // Validate input parameters first
        self.validate_request_parameters(&id, &payload, &user_id)?;

        // Parse and validate thread ID
        let thread_id = self.parse_thread_id(&id)?;

        // Validate thread ownership and lock it
        let thread = self
            .validate_and_lock_thread(thread_id, user_id)
            .await
            .inspect_err(|&e| {
                tracing::warn!("Thread validation failed for user {}: {}", user_id, e);
            })?;

        // Handle user question and validate input
        let user_question = self
            .handle_user_question(&payload, &thread)
            .await
            .inspect_err(|_e| {
                // Ensure thread is unlocked on error
                let connection = self.connection.clone();
                let thread_clone = thread.clone();
                tokio::spawn(async move {
                    Self::ensure_thread_unlocked(&thread_clone, &connection).await;
                });
            })?;

        // Build conversation memory
        let memory = self
            .build_conversation_memory(thread.id)
            .await
            .inspect_err(|&e| {
                tracing::error!(
                    "Failed to build conversation memory for thread {}: {}",
                    thread.id,
                    e
                );
                // Ensure thread is unlocked on error
                let connection = self.connection.clone();
                let thread_clone = thread.clone();
                tokio::spawn(async move {
                    Self::ensure_thread_unlocked(&thread_clone, &connection).await;
                });
            })?;

        // Resolve project path
        let project_path = resolve_project_path().map_err(|e| {
            tracing::error!(
                "Failed to find project path for thread {}: {}",
                thread.id,
                e
            );
            // Ensure thread is unlocked on error
            let connection = self.connection.clone();
            let thread_clone = thread.clone();
            tokio::spawn(async move {
                Self::ensure_thread_unlocked(&thread_clone, &connection).await;
            });
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        // Create streaming persister with better error handling
        let streaming_persister = Arc::new(
            StreamingMessagePersister::new(self.connection.clone(), thread.id, "".to_owned())
                .await
                .map_err(|err| {
                    tracing::error!(
                        "Failed to create streaming message handler for thread {}: {}",
                        thread.id,
                        err
                    );
                    // Ensure thread is unlocked on error
                    let connection = self.connection.clone();
                    let thread_clone = thread.clone();
                    tokio::spawn(async move {
                        Self::ensure_thread_unlocked(&thread_clone, &connection).await;
                    });
                    StatusCode::INTERNAL_SERVER_ERROR
                })?,
        );

        let logs_persister = Arc::new(LogsPersister::new(
            self.connection.clone(),
            user_question.clone(),
            thread.id,
            user_id,
        ));

        let cancellation_tokens = CancellationTokens::new();
        let stream_token = cancellation_tokens.stream_token.clone();

        let execution_context = ChatExecutionContext::new(
            thread.clone(),
            user_question,
            memory,
            project_path,
            streaming_persister,
            logs_persister,
            cancellation_tokens,
        );

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        self.spawn_execution_task(execution_context, tx, executor);

        Ok(Sse::new(create_sse_stream_with_cancellation(
            rx,
            stream_token,
        )))
    }

    fn parse_thread_id(&self, id: &str) -> Result<Uuid, StatusCode> {
        Uuid::parse_str(id).map_err(|e| {
            tracing::warn!("Invalid thread ID format '{}': {}", id, e);
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
                tracing::error!(
                    "Database error finding thread {} for user {}: {}",
                    thread_id,
                    user_id,
                    e
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or_else(|| {
                tracing::warn!(
                    "Thread {} not found or doesn't belong to user {}",
                    thread_id,
                    user_id
                );
                StatusCode::NOT_FOUND
            })?;

        if thread.is_processing {
            tracing::warn!("Thread {} is already being processed", thread_id);
            return Err(StatusCode::CONFLICT);
        }

        // Lock the thread with proper error handling
        let mut thread_model: entity::threads::ActiveModel = thread.clone().into();
        thread_model.is_processing = ActiveValue::Set(true);

        thread_model.update(&self.connection).await.map_err(|e| {
            tracing::error!(
                "Failed to lock thread {} for user {}: {}",
                thread_id,
                user_id,
                e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        tracing::info!(
            "Successfully locked thread {} for user {}",
            thread_id,
            user_id
        );
        Ok(thread)
    }

    async fn handle_user_question<T: ChatExecutionRequest>(
        &self,
        payload: &T,
        thread: &entity::threads::Model,
    ) -> Result<String, StatusCode> {
        let user_question = match payload.get_question() {
            Some(question) => {
                // Validate question content
                if question.trim().is_empty() {
                    tracing::warn!("Empty question provided for thread {}", thread.id);
                    return Err(StatusCode::BAD_REQUEST);
                }

                if question.len() > 10000 {
                    // Reasonable limit
                    tracing::warn!(
                        "Question too long ({} chars) for thread {}",
                        question.len(),
                        thread.id
                    );
                    return Err(StatusCode::BAD_REQUEST);
                }

                let new_message = entity::messages::ActiveModel {
                    id: ActiveValue::Set(Uuid::new_v4()),
                    content: ActiveValue::Set(question.clone()),
                    is_human: ActiveValue::Set(true),
                    thread_id: ActiveValue::Set(thread.id),
                    created_at: ActiveValue::default(),
                    ..Default::default()
                };

                new_message.insert(&self.connection).await.map_err(|e| {
                    tracing::error!(
                        "Failed to insert user message for thread {}: {}",
                        thread.id,
                        e
                    );
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;

                tracing::debug!(
                    "Successfully inserted user message for thread {}",
                    thread.id
                );
                question
            }
            None => {
                // When no question is provided, use the thread's input
                let messages = Messages::find()
                    .filter(entity::messages::Column::ThreadId.eq(thread.id))
                    .limit(1)
                    .all(&self.connection)
                    .await
                    .map_err(|e| {
                        tracing::error!("Failed to fetch messages for thread {}: {}", thread.id, e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })?;

                if messages.len() > 1 {
                    tracing::warn!(
                        "Multiple messages found when expecting none for thread {}",
                        thread.id
                    );
                    return Err(StatusCode::BAD_REQUEST);
                }

                // Validate thread input
                if thread.input.trim().is_empty() {
                    tracing::warn!(
                        "No question provided and thread {} has empty input",
                        thread.id
                    );
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
                tracing::error!(
                    "Failed to fetch conversation history for thread {}: {}",
                    thread_id,
                    e
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        // Sort messages chronologically
        messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        let memory: Vec<Message> = messages
            .into_iter()
            .map(|message| Message {
                content: message.content,
                is_human: message.is_human,
                created_at: message.created_at,
            })
            .collect();

        tracing::debug!(
            "Built conversation memory with {} messages for thread {}",
            memory.len(),
            thread_id
        );
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

                    if let Err(e) = tx.send(cancellation_message).await {
                        tracing::error!("Failed to send cancellation message for thread {}: {}", context.thread.id, e);
                    }
                    stream_token.cancel();
                    if let Err(e) = streaming_persister.cancel("🔴 Operation cancelled").await {
                        tracing::error!("Failed to cancel streaming persister for thread {}: {}", context.thread.id, e);
                    }
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
                        tracing::error!(
                            "Error handling success for thread {}: {}",
                            context.thread.id,
                            e
                        );
                        // Try to send error message to client
                        let error_event = AnswerStream {
                            content: AnswerContent::Error {
                                message: "🔴 Error saving response".to_string(),
                            },
                            references: vec![],
                            is_error: true,
                            step: "system".to_string(),
                        };
                        let _ = tx.send(error_event).await;
                    }
                }
                Err(err) => {
                    if let Err(e) = Self::handle_error(
                        err,
                        streaming_persister.get_message().await,
                        &context.thread,
                        tx.clone(),
                        &connection,
                    )
                    .await
                    {
                        tracing::error!(
                            "Error handling error for thread {}: {}",
                            context.thread.id,
                            e
                        );
                        // Last resort - try to send a generic error
                        let fallback_error = AnswerStream {
                            content: AnswerContent::Error {
                                message: "🔴 An unexpected error occurred".to_string(),
                            },
                            references: vec![],
                            is_error: true,
                            step: "system".to_string(),
                        };
                        let _ = tx.send(fallback_error).await;
                    }
                }
            }

            // Cleanup tasks
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
        // Validate output before saving
        if output.trim().is_empty() {
            tracing::warn!("Empty output generated for thread {}", thread.id);
        }

        let answer_message = entity::messages::ActiveModel {
            id: ActiveValue::Set(message_id),
            content: ActiveValue::Set(output),
            is_human: ActiveValue::Set(false),
            thread_id: ActiveValue::Set(thread.id),
            created_at: ActiveValue::default(),
            input_tokens: ActiveValue::Set(usage.input_tokens.try_into().map_err(|_| {
                OxyError::RuntimeError("Token count conversion failed".to_string())
            })?),
            output_tokens: ActiveValue::Set(usage.output_tokens.try_into().map_err(|_| {
                OxyError::RuntimeError("Token count conversion failed".to_string())
            })?),
        };

        answer_message.update(connection).await.map_err(|err| {
            tracing::error!(
                "Failed to insert agent message for thread {}: {}",
                thread.id,
                err
            );
            OxyError::DBError(format!(
                "Failed to insert agent message for thread {}: {}",
                thread.id, err
            ))
        })?;

        tracing::info!(
            "Successfully saved response for thread {} (input_tokens: {}, output_tokens: {})",
            thread.id,
            usage.input_tokens,
            usage.output_tokens
        );
        Ok(())
    }

    async fn handle_error(
        error: OxyError,
        mut message: messages::ActiveModel,
        thread: &entity::threads::Model,
        tx: tokio::sync::mpsc::Sender<AnswerStream>,
        connection: &sea_orm::DatabaseConnection,
    ) -> Result<(), OxyError> {
        tracing::error!("Error running agent for thread {}: {}", thread.id, error);

        // Create user-friendly error message based on error type
        let user_error_message = match &error {
            OxyError::ValidationError(msg) => format!("🔴 Validation Error: {msg}"),
            OxyError::AuthenticationError(msg) => format!("🔴 Authentication Error: {msg}"),
            OxyError::AuthorizationError(msg) => format!("🔴 Authorization Error: {msg}"),
            OxyError::LLMError(msg) => format!("🔴 LLM Error: {msg}"),
            OxyError::ConfigurationError(msg) => format!("🔴 Configuration Error: {msg}"),
            OxyError::DBError(msg) => format!("🔴 A database error occurred: {msg}"),
            OxyError::RuntimeError(msg) => {
                format!("🔴 An error occurred: {msg}")
            }
            _ => format!("🔴 Error: {error}"),
        };

        let current_content = match message.content.clone().into_value() {
            Some(val) => val,
            None => String::new().into(),
        };

        let current_content_str = match &current_content {
            sea_orm::Value::String(Some(s)) => s.as_str(),
            sea_orm::Value::String(None) => "",
            _ => "",
        };

        let updated_content = format!("{current_content_str}\n{user_error_message}");

        message.content = ActiveValue::Set(updated_content.clone());

        message.update(connection).await.map_err(|err| {
            tracing::error!(
                "Failed to insert error message for thread {}: {}",
                thread.id,
                err
            );
            OxyError::DBError(format!(
                "Failed to insert error message for thread {}: {}",
                thread.id, err
            ))
        })?;

        // Send error event to client
        let error_event = AnswerStream {
            content: AnswerContent::Error {
                message: updated_content.to_string(),
            },
            references: vec![],
            is_error: true,
            step: "error".to_string(),
        };

        tx.send(error_event).await.map_err(|e| {
            tracing::error!(
                "Failed to send error message to client for thread {}: {}",
                thread.id,
                e
            );
            OxyError::RuntimeError(format!(
                "Failed to send error message to client for thread {}: {}",
                thread.id, e
            ))
        })?;

        Ok(())
    }

    async fn unlock_thread(
        thread: &entity::threads::Model,
        connection: &sea_orm::DatabaseConnection,
    ) {
        let mut thread_model: entity::threads::ActiveModel = thread.clone().into();
        thread_model.is_processing = ActiveValue::Set(false);

        match thread_model.update(connection).await {
            Ok(_) => {
                tracing::info!("Successfully unlocked thread {}", thread.id);
            }
            Err(e) => {
                tracing::error!(
                    "Failed to unlock thread {}: {}. This may cause the thread to remain locked.",
                    thread.id,
                    e
                );
                // TODO: we might want to implement a background task
                // to periodically clean up stuck threads.
            }
        }
    }

    /// Helper method to ensure thread is unlocked when operations fail
    async fn ensure_thread_unlocked(
        thread: &entity::threads::Model,
        connection: &sea_orm::DatabaseConnection,
    ) {
        // Only unlock if the thread is currently marked as processing
        if thread.is_processing {
            Self::unlock_thread(thread, connection).await;
        }
    }

    /// Validate input parameters before processing
    fn validate_request_parameters<T: ChatExecutionRequest>(
        &self,
        id: &str,
        payload: &T,
        user_id: &Uuid,
    ) -> Result<(), StatusCode> {
        // Validate thread ID format
        if id.trim().is_empty() {
            tracing::warn!("Empty thread ID provided");
            return Err(StatusCode::BAD_REQUEST);
        }

        // Validate user ID is not nil
        if user_id.is_nil() {
            tracing::warn!("Nil user ID provided");
            return Err(StatusCode::BAD_REQUEST);
        }

        // If question is provided, validate it's not empty
        if let Some(question) = payload.get_question()
            && question.trim().is_empty()
        {
            tracing::warn!("Empty question provided");
            return Err(StatusCode::BAD_REQUEST);
        }

        Ok(())
    }
}
