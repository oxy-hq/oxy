//! JSON-RPC 2.0 protocol types and utilities
//!
//! This module implements the JSON-RPC 2.0 protocol types and helper functions
//! for A2A communication.

use crate::error::JsonRpcError;
use crate::types::{AgentCard, Message, Task, TaskArtifactUpdateEvent, TaskStatusUpdateEvent};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// JSON-RPC 2.0 Request object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// The version of the JSON-RPC protocol. Must be "2.0"
    pub jsonrpc: String,
    /// The name of the method to be invoked
    pub method: String,
    /// The parameter values to be used during the invocation of the method
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<JsonValue>,
    /// An identifier established by the client
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<JsonValue>,
}

impl JsonRpcRequest {
    /// Create a new JSON-RPC request
    pub fn new(
        method: impl Into<String>,
        params: Option<JsonValue>,
        id: Option<JsonValue>,
    ) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
            id,
        }
    }

    /// Check if this is a notification (no id)
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }

    /// Validate the JSON-RPC version
    pub fn validate_version(&self) -> Result<(), String> {
        if self.jsonrpc != "2.0" {
            return Err(format!("Invalid JSON-RPC version: {}", self.jsonrpc));
        }
        Ok(())
    }
}

/// JSON-RPC 2.0 Success Response object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcSuccessResponse {
    /// The version of the JSON-RPC protocol. Must be "2.0"
    pub jsonrpc: String,
    /// The result of the method invocation
    pub result: JsonValue,
    /// The id from the request
    pub id: JsonValue,
}

impl JsonRpcSuccessResponse {
    /// Create a new JSON-RPC success response
    pub fn new(result: JsonValue, id: JsonValue) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result,
            id,
        }
    }
}

/// JSON-RPC 2.0 Error Response object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcErrorResponse {
    /// The version of the JSON-RPC protocol. Must be "2.0"
    pub jsonrpc: String,
    /// The error object
    pub error: JsonRpcError,
    /// The id from the request (or null if there was an error detecting the id)
    pub id: JsonValue,
}

impl JsonRpcErrorResponse {
    /// Create a new JSON-RPC error response
    pub fn new(error: JsonRpcError, id: JsonValue) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            error,
            id,
        }
    }
}

/// JSON-RPC response (either success or error)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcResponse {
    /// Successful response with result
    Success(JsonRpcSuccessResponse),
    /// Error response
    Error(JsonRpcErrorResponse),
}

impl JsonRpcResponse {
    /// Create a success response
    pub fn success(result: JsonValue, id: JsonValue) -> Self {
        JsonRpcResponse::Success(JsonRpcSuccessResponse::new(result, id))
    }

    /// Create an error response
    pub fn error(error: JsonRpcError, id: JsonValue) -> Self {
        JsonRpcResponse::Error(JsonRpcErrorResponse::new(error, id))
    }
}

/// Parameters for the message/send method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSendParams {
    /// The message object being sent to the agent
    pub message: Message,
    /// Optional configuration for the send request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration: Option<MessageSendConfiguration>,
    /// Optional metadata for extensions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Configuration options for message/send or message/stream requests
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

/// Parameters for tasks/get method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskQueryParams {
    /// The unique identifier of the task
    pub id: String,
    /// The number of most recent messages from the task's history to retrieve
    #[serde(skip_serializing_if = "Option::is_none", rename = "historyLength")]
    pub history_length: Option<usize>,
    /// Optional metadata associated with the request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Parameters for tasks/cancel and simple task operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskIdParams {
    /// The unique identifier of the task
    pub id: String,
    /// Optional metadata associated with the request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Parameters for tasks/pushNotificationConfig/get method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskPushNotificationConfigParams {
    /// The unique identifier of the task
    pub id: String,
    /// The ID of the push notification configuration to retrieve
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "pushNotificationConfigId"
    )]
    pub push_notification_config_id: Option<String>,
    /// Optional metadata associated with the request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Parameters for tasks/pushNotificationConfig/list method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTaskPushNotificationConfigParams {
    /// The unique identifier of the task
    pub id: String,
    /// Optional metadata associated with the request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Parameters for tasks/pushNotificationConfig/delete method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteTaskPushNotificationConfigParams {
    /// The unique identifier of the task
    pub id: String,
    /// The ID of the push notification configuration to delete
    #[serde(rename = "pushNotificationConfigId")]
    pub push_notification_config_id: String,
    /// Optional metadata associated with the request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// A container associating a push notification configuration with a specific task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPushNotificationConfig {
    /// The unique identifier of the task
    #[serde(rename = "taskId")]
    pub task_id: String,
    /// The push notification configuration for this task
    #[serde(rename = "pushNotificationConfig")]
    pub push_notification_config: crate::types::PushNotificationConfig,
}

/// Response type for message/stream method (streaming)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SendStreamingMessageResponse {
    /// Successful streaming response
    Success(SendStreamingMessageSuccessResponse),
    /// Error response
    Error(JsonRpcErrorResponse),
}

/// Success response for message/stream method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendStreamingMessageSuccessResponse {
    /// The version of the JSON-RPC protocol. Must be "2.0"
    pub jsonrpc: String,
    /// The result, which can be a Message, Task, or a streaming update event
    pub result: StreamingResult,
    /// The id from the request
    pub id: JsonValue,
}

/// Result type for streaming responses
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StreamingResult {
    /// Message event
    Message(Message),
    /// Task event
    Task(Task),
    /// Task status update event
    StatusUpdate(TaskStatusUpdateEvent),
    /// Task artifact update event
    ArtifactUpdate(TaskArtifactUpdateEvent),
}

/// Response for agent/getAuthenticatedExtendedCard method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAuthenticatedExtendedCardResponse {
    /// The version of the JSON-RPC protocol. Must be "2.0"
    pub jsonrpc: String,
    /// The result is an Agent Card object
    pub result: AgentCard,
    /// The id from the request
    pub id: JsonValue,
}

/// Helper function to parse JSON-RPC request
pub fn parse_request(json: &str) -> Result<JsonRpcRequest, JsonRpcError> {
    serde_json::from_str(json).map_err(|e| JsonRpcError::parse_error(e.to_string()))
}

/// Helper function to serialize JSON-RPC response
pub fn serialize_response(response: &JsonRpcResponse) -> Result<String, JsonRpcError> {
    serde_json::to_string(response).map_err(|e| JsonRpcError::internal_error(e.to_string()))
}

/// Extract params as a specific type from a JSON-RPC request
pub fn extract_params<T: for<'de> Deserialize<'de>>(
    request: &JsonRpcRequest,
) -> Result<T, JsonRpcError> {
    match &request.params {
        Some(params) => serde_json::from_value(params.clone())
            .map_err(|e| JsonRpcError::invalid_params(format!("Failed to parse params: {}", e))),
        None => Err(JsonRpcError::invalid_params("Missing required params")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_request_creation() {
        let request = JsonRpcRequest::new(
            "message/send",
            Some(serde_json::json!({"test": "data"})),
            Some(serde_json::json!(1)),
        );

        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.method, "message/send");
        assert!(request.params.is_some());
        assert!(request.id.is_some());
        assert!(!request.is_notification());
    }

    #[test]
    fn test_json_rpc_notification() {
        let request = JsonRpcRequest::new("test/method", None, None);
        assert!(request.is_notification());
    }

    #[test]
    fn test_json_rpc_success_response() {
        let response = JsonRpcResponse::success(
            serde_json::json!({"result": "success"}),
            serde_json::json!(1),
        );

        match response {
            JsonRpcResponse::Success(success) => {
                assert_eq!(success.jsonrpc, "2.0");
                assert_eq!(success.id, serde_json::json!(1));
            }
            _ => panic!("Expected success response"),
        }
    }

    #[test]
    fn test_json_rpc_error_response() {
        let error = JsonRpcError::method_not_found("test/method");
        let response = JsonRpcResponse::error(error, serde_json::json!(1));

        match response {
            JsonRpcResponse::Error(error_response) => {
                assert_eq!(error_response.jsonrpc, "2.0");
                assert_eq!(error_response.error.code, -32601);
            }
            _ => panic!("Expected error response"),
        }
    }
}
