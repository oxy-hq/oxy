use std::sync::{PoisonError, TryLockError};

use async_openai::error::OpenAIError;
use axum::http::StatusCode;
use thiserror::Error;
use tokio::{sync::mpsc::error::SendError, task::JoinError};

#[derive(Error, Debug)]
pub enum OxyError {
    #[error("Invalid configuration:\n{0}")]
    ConfigurationError(String),
    #[error("Invalid argument:\n{0}")]
    ArgumentError(String),
    #[error("Runtime error:\n{0}")]
    RuntimeError(String),
    #[error("LLM error:\n{0}")]
    LLMError(String),
    #[error("Agent error:\n{0}")]
    AgentError(String),
    #[error("Anonymizer error:\n{0}")]
    AnonymizerError(String),
    #[error("Serializer error:\n{0}")]
    SerializerError(String),
    #[error("IO error:\n{0}")]
    IOError(String),
    #[error("DB error:\n{0}")]
    DBError(String),
    #[error("LanceDB error:\n{0}")]
    LanceDBError(#[from] lancedb::Error),
    #[error("SerdeArrow error:\n{0}")]
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
        OxyError::RuntimeError(format!("Error in completion request: {:?}", value))
    }
}
