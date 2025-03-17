use std::sync::PoisonError;

use axum::http::StatusCode;
use thiserror::Error;
use tokio::{sync::mpsc::error::SendError, task::JoinError};

#[derive(Error, Debug, Clone)]
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
        }
    }
}
