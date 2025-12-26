//! # A2A (Agent-to-Agent) Protocol Implementation
//!
//! This crate provides a standalone implementation of the A2A protocol,
//! a standard for communication and interoperability between independent AI agent systems.
//!
//! ## Overview
//!
//! The A2A protocol enables agents to:
//! - Discover each other's capabilities through Agent Cards
//! - Exchange messages with rich content (text, files, structured data)
//! - Manage collaborative tasks with stateful lifecycle
//! - Stream real-time updates via Server-Sent Events (SSE)
//! - Communicate using JSON-RPC 2.0, gRPC, or HTTP+JSON transports
//!
//! ## Features
//!
//! - **Protocol-Agnostic**: Core types with no dependencies on specific frameworks
//! - **Multiple Transports**: Support for JSON-RPC, HTTP+JSON, and gRPC
//! - **Streaming**: SSE (Server-Sent Events) for real-time updates
//! - **Type-Safe**: Strongly-typed Rust implementations of all A2A data structures
//! - **Validation**: Built-in validation for protocol compliance
//! - **Error Handling**: Comprehensive error types mapping to JSON-RPC error codes
//!
//! ## Usage
//!
//! ### Basic Types
//!
//! ```rust
//! use a2a::types::{Message, Part, TextPart, MessageRole};
//!
//! // Create a message
//! let message = Message::new_user(vec![
//!     Part::Text(TextPart::new("Hello, agent!"))
//! ]);
//! ```
//!
//! ### JSON-RPC Communication
//!
//! ```rust
//! use a2a::jsonrpc::JsonRpcRequest;
//!
//! // Create a JSON-RPC request
//! let request = JsonRpcRequest::new(
//!     "message/send",
//!     Some(serde_json::json!({"message": {"role": "user", "parts": []}})),
//!     Some(serde_json::json!(1)),
//! );
//! ```
//!
//! ### SSE Streaming
//!
//! ```rust
//! use a2a::streaming::{SseEvent, SseEventType};
//!
//! // Create a streaming event
//! let event = SseEvent::with_type(
//!     SseEventType::TaskStatusUpdate,
//!     "{\"taskId\": \"123\", \"status\": \"working\"}"
//! );
//! ```
//!
//! ## Modules
//!
//! - [`types`]: Core A2A data structures (Message, Task, AgentCard, etc.)
//! - [`error`]: Error types and result aliases
//! - [`jsonrpc`]: JSON-RPC 2.0 protocol types and utilities
//! - [`http`]: HTTP+JSON protocol types and utilities
//! - [`streaming`]: Server-Sent Events (SSE) streaming utilities
//! - [`validation`]: Protocol validation functions
//! - [`storage`]: Task storage abstraction and implementations
//! - [`server`]: Server abstractions and handler traits
//!
//! ## Protocol Version
//!
//! This implementation supports A2A protocol version **0.3.0**.

#![deny(missing_docs)]
#![warn(clippy::all)]

pub mod error;
pub mod http;
pub mod jsonrpc;
pub mod server;
pub mod storage;
pub mod streaming;
pub mod types;
pub mod validation;

// Re-export commonly used types for convenience
pub use error::{A2aError, A2aResult, JsonRpcError};
pub use server::{A2aContext, A2aHandler, SseStream, create_http_router, create_jsonrpc_router};
pub use storage::{InMemoryTaskStorage, TaskFilters, TaskStorage};
pub use types::{
    AgentCapabilities, AgentCard, AgentSkill, Artifact, Message, MessageRole, Part, Task,
    TaskState, TaskStatus, TextPart, TransportProtocol,
};

/// A2A protocol version supported by this crate
pub const PROTOCOL_VERSION: &str = "0.3.0";

/// Default JSON-RPC version
pub const JSONRPC_VERSION: &str = "2.0";

/// Prelude module for convenient imports
pub mod prelude {
    //! Convenient re-exports of commonly used types and traits
    pub use crate::error::{A2aError, A2aResult, JsonRpcError};
    pub use crate::http::{HttpErrorResponse, HttpMethod, HttpStatus};
    pub use crate::jsonrpc::{
        JsonRpcRequest, JsonRpcResponse, MessageSendConfiguration, MessageSendParams, TaskIdParams,
        TaskQueryParams,
    };
    pub use crate::server::{
        A2aContext, A2aHandler, SseStream, create_http_router, create_jsonrpc_router,
    };
    pub use crate::storage::{InMemoryTaskStorage, TaskFilters, TaskStorage};
    pub use crate::streaming::{SseEvent, SseEventType, SseStreamBuilder};
    pub use crate::types::{
        AgentCapabilities, AgentCard, AgentInterface, AgentSkill, Artifact, DataPart, FilePart,
        Message, MessageRole, Part, Task, TaskArtifactUpdateEvent, TaskState, TaskStatus,
        TaskStatusUpdateEvent, TextPart, TransportProtocol,
    };
    pub use crate::validation::{
        is_terminal_state, validate_agent_card, validate_message, validate_task,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version() {
        assert_eq!(PROTOCOL_VERSION, "0.3.0");
    }

    #[test]
    fn test_jsonrpc_version() {
        assert_eq!(JSONRPC_VERSION, "2.0");
    }
}
