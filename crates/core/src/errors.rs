use std::sync::{PoisonError, TryLockError};

use async_openai::error::OpenAIError;
use axum::http::StatusCode;
use thiserror::Error;
use tokio::{sync::mpsc::error::SendError, task::JoinError};

#[derive(Error, Debug)]
pub enum OxyError {
    #[error("{0}")]
    ConfigurationError(String),
    #[error("{0}")]
    ArgumentError(String),
    #[error("{0}")]
    RuntimeError(String),
    #[error("{0}")]
    LLMError(String),
    #[error("{0}")]
    AgentError(String),
    #[error("{0}")]
    AnonymizerError(String),
    #[error("{0}")]
    SerializerError(String),
    #[error("{0}")]
    IOError(String),
    #[error("{0}")]
    DBError(String),
    #[error("{0}")]
    Database(String),
    #[error("{0}")]
    SecretManager(String),
    #[error("{0:?}")]
    SecretNotFound(Option<String>),
    #[error("{0}")]
    AuthenticationError(String),
    #[error("{0}")]
    AuthorizationError(String),
    #[error("{0}")]
    ValidationError(String),
    #[error("{0}")]
    CryptographyError(String),
    #[error("{0}")]
    InitializationError(String),
    #[error("{0}")]
    JobError(String),
    #[error("{0}")]
    LanceDBError(#[from] lancedb::Error),
    #[error("{0}")]
    SerdeArrowError(#[from] serde_arrow::Error),
    #[error("Error when calling {handle}:\n{msg}")]
    ToolCallError {
        call_id: String,
        handle: String,
        param: String,
        msg: String,
    },
}

impl From<Box<dyn std::error::Error>> for OxyError {
    fn from(error: Box<dyn std::error::Error>) -> Self {
        OxyError::RuntimeError(error.to_string())
    }
}

impl From<anyhow::Error> for OxyError {
    fn from(error: anyhow::Error) -> Self {
        OxyError::RuntimeError(error.to_string())
    }
}

impl From<String> for OxyError {
    fn from(error: String) -> Self {
        OxyError::RuntimeError(error)
    }
}

impl<T> From<PoisonError<T>> for OxyError {
    fn from(error: PoisonError<T>) -> Self {
        OxyError::RuntimeError(format!("Failed to acquire lock: {error}"))
    }
}

impl<T> From<TryLockError<T>> for OxyError {
    fn from(error: TryLockError<T>) -> Self {
        OxyError::RuntimeError(format!("Failed to acquire lock: {error}"))
    }
}

impl From<serde_json::Error> for OxyError {
    fn from(error: serde_json::Error) -> Self {
        OxyError::SerializerError(error.to_string())
    }
}

impl<Event> From<SendError<Event>> for OxyError {
    fn from(error: SendError<Event>) -> Self {
        OxyError::RuntimeError(format!("Failed to send event: {error}"))
    }
}

impl From<JoinError> for OxyError {
    fn from(error: JoinError) -> Self {
        OxyError::RuntimeError(format!("Failed to join task: {error}"))
    }
}

impl From<std::io::Error> for OxyError {
    fn from(error: std::io::Error) -> Self {
        OxyError::IOError(error.to_string())
    }
}

impl From<OxyError> for StatusCode {
    fn from(error: OxyError) -> Self {
        tracing::error!("Error occurred: {}", error);
        match error {
            OxyError::ConfigurationError(_) => StatusCode::BAD_REQUEST,
            OxyError::ArgumentError(_) => StatusCode::BAD_REQUEST,
            OxyError::RuntimeError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            OxyError::LLMError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            OxyError::AgentError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            OxyError::AnonymizerError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            OxyError::SerializerError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            OxyError::IOError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            OxyError::DBError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            OxyError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            OxyError::SecretManager(_) => StatusCode::INTERNAL_SERVER_ERROR,
            OxyError::SecretNotFound(_) => StatusCode::INTERNAL_SERVER_ERROR,
            OxyError::AuthenticationError(_) => StatusCode::UNAUTHORIZED,
            OxyError::AuthorizationError(_) => StatusCode::FORBIDDEN,
            OxyError::ValidationError(_) => StatusCode::BAD_REQUEST,
            OxyError::CryptographyError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            OxyError::InitializationError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            OxyError::JobError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            OxyError::LanceDBError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            OxyError::SerdeArrowError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            OxyError::ToolCallError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

const CONTEXT_WINDOW_EXCEEDED_CODE: &str = "string_above_max_length";

impl From<OpenAIError> for OxyError {
    fn from(value: OpenAIError) -> Self {
        if let OpenAIError::ApiError(ref api_error) = value {
            if api_error.code == Some(CONTEXT_WINDOW_EXCEEDED_CODE.to_string()) {
                return OxyError::LLMError(
                    "Context window length exceeded. Shorten the prompt being sent to the LLM."
                        .into(),
                );
            }
        }
        OxyError::RuntimeError(format!("Error in completion request: {value:?}"))
    }
}
