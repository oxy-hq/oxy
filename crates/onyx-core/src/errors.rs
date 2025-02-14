use std::sync::PoisonError;

use thiserror::Error;
use tokio::{sync::mpsc::error::SendError, task::JoinError};

#[derive(Error, Debug, Clone)]
pub enum OnyxError {
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
}

impl From<Box<dyn std::error::Error>> for OnyxError {
    fn from(error: Box<dyn std::error::Error>) -> Self {
        OnyxError::RuntimeError(error.to_string())
    }
}

impl From<anyhow::Error> for OnyxError {
    fn from(error: anyhow::Error) -> Self {
        OnyxError::RuntimeError(error.to_string())
    }
}

impl From<String> for OnyxError {
    fn from(error: String) -> Self {
        OnyxError::RuntimeError(error)
    }
}

impl<T> From<PoisonError<T>> for OnyxError {
    fn from(error: PoisonError<T>) -> Self {
        OnyxError::RuntimeError(format!("Failed to acquire lock: {error}"))
    }
}

impl From<serde_json::Error> for OnyxError {
    fn from(error: serde_json::Error) -> Self {
        OnyxError::SerializerError(error.to_string())
    }
}

impl<Event> From<SendError<Event>> for OnyxError {
    fn from(error: SendError<Event>) -> Self {
        OnyxError::RuntimeError(format!("Failed to send event: {error}"))
    }
}

impl From<JoinError> for OnyxError {
    fn from(error: JoinError) -> Self {
        OnyxError::RuntimeError(format!("Failed to join task: {error}"))
    }
}
