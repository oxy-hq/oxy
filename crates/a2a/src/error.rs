//! A2A protocol error types
//!
//! This module defines error types for A2A protocol operations, including
//! both standard JSON-RPC errors and A2A-specific error codes.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A2A protocol error
#[derive(Debug, Error)]
pub enum A2aError {
    /// Parse error - Invalid JSON payload
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Invalid Request - Invalid JSON-RPC Request
    #[error("Invalid Request: {0}")]
    InvalidRequest(String),

    /// Method not found
    #[error("Method not found: {0}")]
    MethodNotFound(String),

    /// Invalid params - Invalid method parameters
    #[error("Invalid params: {0}")]
    InvalidParams(String),

    /// Internal error - Internal server error
    #[error("Internal error: {0}")]
    InternalError(String),

    /// Task not found
    #[error("Task not found: {0}")]
    TaskNotFound(String),

    /// Task cannot be canceled
    #[error("Task cannot be canceled: {0}")]
    TaskNotCancelable(String),

    /// Push notification not supported
    #[error("Push notification not supported")]
    PushNotificationNotSupported,

    /// Unsupported operation
    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),

    /// Content type not supported
    #[error("Content type not supported: {0}")]
    ContentTypeNotSupported(String),

    /// Invalid agent response
    #[error("Invalid agent response: {0}")]
    InvalidAgentResponse(String),

    /// Invalid task
    #[error("Invalid task: {0}")]
    InvalidTask(String),

    /// Unauthorized
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Forbidden
    #[error("Forbidden: {0}")]
    Forbidden(String),

    /// Authenticated extended card not configured
    #[error("Authenticated extended card not configured")]
    AuthenticatedExtendedCardNotConfigured,

    /// Custom server error
    #[error("Server error: {0}")]
    ServerError(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Validation error
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// Storage error - Database or persistence error
    #[error("Storage error: {0}")]
    StorageError(String),
}

impl A2aError {
    /// Get the JSON-RPC error code for this error
    pub fn code(&self) -> i32 {
        match self {
            A2aError::ParseError(_) => -32700,
            A2aError::InvalidRequest(_) => -32600,
            A2aError::MethodNotFound(_) => -32601,
            A2aError::InvalidParams(_) => -32602,
            A2aError::InternalError(_) => -32603,
            A2aError::TaskNotFound(_) => -32001,
            A2aError::TaskNotCancelable(_) => -32002,
            A2aError::InvalidTask(_) => -32602,
            A2aError::Unauthorized(_) => -32008,
            A2aError::Forbidden(_) => -32009,
            A2aError::PushNotificationNotSupported => -32003,
            A2aError::UnsupportedOperation(_) => -32004,
            A2aError::ContentTypeNotSupported(_) => -32005,
            A2aError::InvalidAgentResponse(_) => -32006,
            A2aError::AuthenticatedExtendedCardNotConfigured => -32007,
            A2aError::ServerError(_) => -32000,
            A2aError::SerializationError(_) => -32603,
            A2aError::ValidationError(_) => -32602,
            A2aError::StorageError(_) => -32603,
        }
    }

    /// Get the error message
    pub fn message(&self) -> String {
        self.to_string()
    }

    /// Convert to JSON-RPC error object
    pub fn to_jsonrpc_error(&self) -> JsonRpcError {
        JsonRpcError {
            code: self.code(),
            message: self.message(),
            data: None,
        }
    }

    /// Convert to JSON-RPC error object with additional data
    pub fn to_jsonrpc_error_with_data(&self, data: serde_json::Value) -> JsonRpcError {
        JsonRpcError {
            code: self.code(),
            message: self.message(),
            data: Some(data),
        }
    }
}

impl From<serde_json::Error> for A2aError {
    fn from(err: serde_json::Error) -> Self {
        A2aError::SerializationError(err.to_string())
    }
}

/// JSON-RPC 2.0 Error object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// A number that indicates the error type that occurred
    pub code: i32,
    /// A string providing a short description of the error
    pub message: String,
    /// A primitive or structured value containing additional information about the error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    /// Create a new JSON-RPC error
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Create a new JSON-RPC error with data
    pub fn with_data(code: i32, message: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            code,
            message: message.into(),
            data: Some(data),
        }
    }

    /// Create a parse error
    pub fn parse_error(message: impl Into<String>) -> Self {
        Self::new(-32700, message)
    }

    /// Create an invalid request error
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(-32600, message)
    }

    /// Create a method not found error
    pub fn method_not_found(method: impl Into<String>) -> Self {
        Self::new(-32601, format!("Method not found: {}", method.into()))
    }

    /// Create an invalid params error
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(-32602, message)
    }

    /// Create an internal error
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new(-32603, message)
    }

    /// Create a task not found error
    pub fn task_not_found(task_id: impl Into<String>) -> Self {
        Self::new(-32001, format!("Task not found: {}", task_id.into()))
    }

    /// Create a task not cancelable error
    pub fn task_not_cancelable(task_id: impl Into<String>) -> Self {
        Self::new(
            -32002,
            format!("Task cannot be canceled: {}", task_id.into()),
        )
    }

    /// Create a push notification not supported error
    pub fn push_notification_not_supported() -> Self {
        Self::new(-32003, "Push notification not supported")
    }

    /// Create an unsupported operation error
    pub fn unsupported_operation(message: impl Into<String>) -> Self {
        Self::new(-32004, message)
    }

    /// Create a content type not supported error
    pub fn content_type_not_supported(content_type: impl Into<String>) -> Self {
        Self::new(
            -32005,
            format!("Content type not supported: {}", content_type.into()),
        )
    }

    /// Create an invalid agent response error
    pub fn invalid_agent_response(message: impl Into<String>) -> Self {
        Self::new(-32006, message)
    }

    /// Create an authenticated extended card not configured error
    pub fn authenticated_extended_card_not_configured() -> Self {
        Self::new(-32007, "Authenticated extended card not configured")
    }

    /// Convert from A2aError
    pub fn from_a2a_error(err: A2aError) -> Self {
        err.to_jsonrpc_error()
    }
}

impl From<A2aError> for JsonRpcError {
    fn from(err: A2aError) -> Self {
        err.to_jsonrpc_error()
    }
}

/// Result type for A2A operations
pub type A2aResult<T> = Result<T, A2aError>;
