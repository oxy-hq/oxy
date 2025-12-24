//! HTTP+JSON/REST protocol utilities
//!
//! This module provides utilities for HTTP+JSON/REST-style A2A communication,
//! including request/response types and helper functions.

use crate::error::{A2aError, JsonRpcError};
use crate::types::{AgentCard, Message, Task};
use serde::{Deserialize, Serialize};

#[allow(unused_imports)]
use crate::error::A2aResult;

/// HTTP method types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    /// HTTP GET method
    Get,
    /// HTTP POST method
    Post,
    /// HTTP PUT method
    Put,
    /// HTTP DELETE method
    Delete,
}

impl HttpMethod {
    /// Convert HTTP method to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
        }
    }
}

/// HTTP status codes commonly used in A2A
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpStatus {
    /// 200 OK
    Ok = 200,
    /// 201 Created
    Created = 201,
    /// 204 No Content
    NoContent = 204,
    /// 400 Bad Request
    BadRequest = 400,
    /// 401 Unauthorized
    Unauthorized = 401,
    /// 403 Forbidden
    Forbidden = 403,
    /// 404 Not Found
    NotFound = 404,
    /// 500 Internal Server Error
    InternalServerError = 500,
}

impl HttpStatus {
    /// Get the numeric HTTP status code
    pub fn code(&self) -> u16 {
        *self as u16
    }

    /// Check if this is a success status code (2xx)
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            HttpStatus::Ok | HttpStatus::Created | HttpStatus::NoContent
        )
    }

    /// Check if this is an error status code (4xx or 5xx)
    pub fn is_error(&self) -> bool {
        !self.is_success()
    }
}

/// HTTP error response body
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpErrorResponse {
    /// Error code (matches JSON-RPC error codes)
    pub code: i32,
    /// Error message
    pub message: String,
    /// Optional additional error data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl HttpErrorResponse {
    /// Create a new HTTP error response
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Add additional data to the error response
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    /// Convert a JSON-RPC error to an HTTP error response
    pub fn from_jsonrpc_error(error: JsonRpcError) -> Self {
        Self {
            code: error.code,
            message: error.message,
            data: error.data,
        }
    }

    /// Convert this HTTP error response to a JSON-RPC error
    pub fn to_jsonrpc_error(&self) -> JsonRpcError {
        JsonRpcError {
            code: self.code,
            message: self.message.clone(),
            data: self.data.clone(),
        }
    }
}

impl From<A2aError> for HttpErrorResponse {
    fn from(err: A2aError) -> Self {
        Self::from_jsonrpc_error(err.to_jsonrpc_error())
    }
}

impl From<JsonRpcError> for HttpErrorResponse {
    fn from(err: JsonRpcError) -> Self {
        Self::from_jsonrpc_error(err)
    }
}

/// Request body for POST /v1/message:send
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    /// The message object being sent to the agent
    pub message: Message,
    /// Optional target skill to invoke
    #[serde(skip_serializing_if = "Option::is_none", rename = "targetSkill")]
    pub target_skill: Option<String>,
    /// Optional configuration for the send request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration: Option<MessageSendConfiguration>,
}

/// Request body for POST /v1/message:stream
pub type StreamMessageRequest = SendMessageRequest;

/// Configuration options for message sending
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSendConfiguration {
    /// A list of output MIME types the client is prepared to accept in the response
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "acceptedOutputModes"
    )]
    pub accepted_output_modes: Option<Vec<String>>,
    /// The number of most recent messages from the task's history to retrieve in the response
    #[serde(skip_serializing_if = "Option::is_none", rename = "historyLength")]
    pub history_length: Option<usize>,
    /// Configuration for the agent to send push notifications for updates after the initial response
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "pushNotificationConfig"
    )]
    pub push_notification_config: Option<crate::types::PushNotificationConfig>,
    /// If true, the client will wait for the task to complete
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<bool>,
}

/// Response body for POST /v1/message:send
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageResponse {
    /// The task representing the message interaction
    pub task: Task,
}

/// Query parameters for GET /v1/tasks/{id}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskQueryParams {
    /// The number of most recent messages from the task's history to retrieve
    #[serde(skip_serializing_if = "Option::is_none", rename = "historyLength")]
    pub history_length: Option<usize>,
}

/// Response body for GET /v1/tasks/{id}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskResponse {
    /// The task object
    pub task: Task,
}

/// Response body for POST /v1/tasks/{id}:cancel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelTaskResponse {
    /// The canceled task object
    pub task: Task,
}

/// Request body for POST /v1/tasks/{id}/pushNotificationConfigs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePushNotificationConfigRequest {
    /// The push notification configuration
    #[serde(rename = "pushNotificationConfig")]
    pub push_notification_config: crate::types::PushNotificationConfig,
}

/// Response body for POST /v1/tasks/{id}/pushNotificationConfigs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePushNotificationConfigResponse {
    /// The task ID
    #[serde(rename = "taskId")]
    pub task_id: String,
    /// The push notification configuration
    #[serde(rename = "pushNotificationConfig")]
    pub push_notification_config: crate::types::PushNotificationConfig,
}

/// Response body for GET /v1/tasks/{id}/pushNotificationConfigs/{configId}
pub type GetPushNotificationConfigResponse = CreatePushNotificationConfigResponse;

/// Response body for GET /v1/tasks/{id}/pushNotificationConfigs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListPushNotificationConfigsResponse {
    /// List of push notification configurations
    pub configs: Vec<crate::types::PushNotificationConfig>,
}

/// Response body for GET /v1/card
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAgentCardResponse {
    /// The agent card
    pub card: AgentCard,
}

/// Helper to map HTTP status codes to A2A errors
pub fn http_status_to_error(status: HttpStatus, message: Option<String>) -> A2aError {
    let msg = message.unwrap_or_else(|| "HTTP error".to_string());
    match status {
        HttpStatus::BadRequest => A2aError::InvalidRequest(msg),
        HttpStatus::Unauthorized => A2aError::Unauthorized(msg),
        HttpStatus::Forbidden => A2aError::Forbidden(msg),
        HttpStatus::NotFound => A2aError::TaskNotFound(msg),
        HttpStatus::InternalServerError => A2aError::InternalError(msg),
        _ => A2aError::ServerError(msg),
    }
}

/// Helper to map A2A errors to HTTP status codes
pub fn error_to_http_status(error: &A2aError) -> HttpStatus {
    match error {
        A2aError::ParseError(_) | A2aError::InvalidRequest(_) | A2aError::InvalidParams(_) => {
            HttpStatus::BadRequest
        }
        A2aError::TaskNotFound(_) => HttpStatus::NotFound,
        A2aError::MethodNotFound(_) => HttpStatus::NotFound,
        A2aError::TaskNotCancelable(_) => HttpStatus::BadRequest,
        A2aError::InvalidTask(_) => HttpStatus::BadRequest,
        A2aError::PushNotificationNotSupported => HttpStatus::BadRequest,
        A2aError::UnsupportedOperation(_) => HttpStatus::BadRequest,
        A2aError::ContentTypeNotSupported(_) => HttpStatus::BadRequest,
        A2aError::InvalidAgentResponse(_) => HttpStatus::InternalServerError,
        A2aError::AuthenticatedExtendedCardNotConfigured => HttpStatus::NotFound,
        A2aError::InternalError(_) | A2aError::ServerError(_) | A2aError::SerializationError(_) => {
            HttpStatus::InternalServerError
        }
        A2aError::ValidationError(_) => HttpStatus::BadRequest,
        A2aError::StorageError(_) => HttpStatus::InternalServerError,
        A2aError::Unauthorized(_) => HttpStatus::Unauthorized,
        A2aError::Forbidden(_) => HttpStatus::Forbidden,
    }
}

/// REST endpoint paths for A2A
pub mod paths {
    /// Base path for A2A endpoints
    pub const BASE: &str = "/v1";

    /// POST /v1/message:send
    pub const MESSAGE_SEND: &str = "/v1/message:send";

    /// POST /v1/message:stream
    pub const MESSAGE_STREAM: &str = "/v1/message:stream";

    /// GET /v1/tasks/{id}
    pub fn task_get(task_id: &str) -> String {
        format!("/v1/tasks/{}", task_id)
    }

    /// POST /v1/tasks/{id}:cancel
    pub fn task_cancel(task_id: &str) -> String {
        format!("/v1/tasks/{}:cancel", task_id)
    }

    /// POST /v1/tasks/{id}:subscribe
    pub fn task_subscribe(task_id: &str) -> String {
        format!("/v1/tasks/{}:subscribe", task_id)
    }

    /// POST /v1/tasks/{id}/pushNotificationConfigs
    pub fn push_notification_create(task_id: &str) -> String {
        format!("/v1/tasks/{}/pushNotificationConfigs", task_id)
    }

    /// GET /v1/tasks/{id}/pushNotificationConfigs/{configId}
    pub fn push_notification_get(task_id: &str, config_id: &str) -> String {
        format!(
            "/v1/tasks/{}/pushNotificationConfigs/{}",
            task_id, config_id
        )
    }

    /// GET /v1/tasks/{id}/pushNotificationConfigs
    pub fn push_notification_list(task_id: &str) -> String {
        format!("/v1/tasks/{}/pushNotificationConfigs", task_id)
    }

    /// DELETE /v1/tasks/{id}/pushNotificationConfigs/{configId}
    pub fn push_notification_delete(task_id: &str, config_id: &str) -> String {
        format!(
            "/v1/tasks/{}/pushNotificationConfigs/{}",
            task_id, config_id
        )
    }

    /// GET /v1/card
    pub const CARD: &str = "/v1/card";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_status_code() {
        assert_eq!(HttpStatus::Ok.code(), 200);
        assert_eq!(HttpStatus::NotFound.code(), 404);
        assert_eq!(HttpStatus::InternalServerError.code(), 500);
    }

    #[test]
    fn test_http_status_is_success() {
        assert!(HttpStatus::Ok.is_success());
        assert!(HttpStatus::Created.is_success());
        assert!(!HttpStatus::BadRequest.is_success());
        assert!(!HttpStatus::InternalServerError.is_success());
    }

    #[test]
    fn test_error_to_http_status() {
        assert_eq!(
            error_to_http_status(&A2aError::TaskNotFound("test".to_string())),
            HttpStatus::NotFound
        );
        assert_eq!(
            error_to_http_status(&A2aError::InvalidRequest("test".to_string())),
            HttpStatus::BadRequest
        );
        assert_eq!(
            error_to_http_status(&A2aError::InternalError("test".to_string())),
            HttpStatus::InternalServerError
        );
    }

    #[test]
    fn test_paths() {
        assert_eq!(paths::MESSAGE_SEND, "/v1/message:send");
        assert_eq!(paths::task_get("123"), "/v1/tasks/123");
        assert_eq!(paths::task_cancel("123"), "/v1/tasks/123:cancel");
        assert_eq!(
            paths::push_notification_get("123", "456"),
            "/v1/tasks/123/pushNotificationConfigs/456"
        );
    }
}
