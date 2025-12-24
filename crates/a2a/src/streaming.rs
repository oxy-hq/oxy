//! Server-Sent Events (SSE) streaming utilities
//!
//! This module provides types and utilities for SSE streaming in A2A protocol,
//! enabling real-time task updates via Server-Sent Events.

use crate::jsonrpc::SendStreamingMessageResponse;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Content-Type header value for SSE streams
pub const SSE_CONTENT_TYPE: &str = "text/event-stream";

/// SSE event types used in A2A streaming
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SseEventType {
    /// Task created event
    TaskCreated,
    /// Task progress update
    TaskProgress,
    /// Task status changed
    TaskStatusUpdate,
    /// Artifact created or updated
    ArtifactUpdate,
    /// Task completed
    TaskCompleted,
    /// Task failed
    TaskFailed,
    /// Message from agent
    Message,
    /// Generic data event
    Data,
    /// Keep-alive/heartbeat event
    Heartbeat,
    /// Error event
    Error,
}

impl fmt::Display for SseEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SseEventType::TaskCreated => "task-created",
            SseEventType::TaskProgress => "task-progress",
            SseEventType::TaskStatusUpdate => "task-status-update",
            SseEventType::ArtifactUpdate => "artifact-update",
            SseEventType::TaskCompleted => "task-completed",
            SseEventType::TaskFailed => "task-failed",
            SseEventType::Message => "message",
            SseEventType::Data => "data",
            SseEventType::Heartbeat => "heartbeat",
            SseEventType::Error => "error",
        };
        write!(f, "{}", s)
    }
}

/// A Server-Sent Event
#[derive(Debug, Clone)]
pub struct SseEvent {
    /// Event type (optional, defaults to "message" in SSE spec)
    pub event_type: Option<SseEventType>,
    /// Event data (required)
    pub data: String,
    /// Event ID (optional, for client resume capability)
    pub id: Option<String>,
    /// Retry time in milliseconds (optional)
    pub retry: Option<u32>,
    /// Comment (optional, ignored by clients but useful for debugging)
    pub comment: Option<String>,
}

impl SseEvent {
    /// Create a new SSE event with data
    pub fn new(data: impl Into<String>) -> Self {
        Self {
            event_type: None,
            data: data.into(),
            id: None,
            retry: None,
            comment: None,
        }
    }

    /// Create a new SSE event with event type and data
    pub fn with_type(event_type: SseEventType, data: impl Into<String>) -> Self {
        Self {
            event_type: Some(event_type),
            data: data.into(),
            id: None,
            retry: None,
            comment: None,
        }
    }

    /// Set the event ID
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the retry time
    pub fn with_retry(mut self, retry: u32) -> Self {
        self.retry = Some(retry);
        self
    }

    /// Set a comment
    pub fn with_comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = Some(comment.into());
        self
    }

    /// Format the event according to SSE specification
    pub fn format(&self) -> String {
        let mut output = String::new();

        // Add comment if present
        if let Some(comment) = &self.comment {
            for line in comment.lines() {
                output.push_str(&format!(": {}\n", line));
            }
        }

        // Add event type if present
        if let Some(event_type) = &self.event_type {
            output.push_str(&format!("event: {}\n", event_type));
        }

        // Add ID if present
        if let Some(id) = &self.id {
            output.push_str(&format!("id: {}\n", id));
        }

        // Add retry if present
        if let Some(retry) = self.retry {
            output.push_str(&format!("retry: {}\n", retry));
        }

        // Add data (required)
        for line in self.data.lines() {
            output.push_str(&format!("data: {}\n", line));
        }

        // SSE events end with double newline
        output.push('\n');

        output
    }

    /// Create a heartbeat/keep-alive event
    pub fn heartbeat() -> Self {
        Self::with_type(SseEventType::Heartbeat, "{}")
    }

    /// Create an error event
    pub fn error(error_message: impl Into<String>) -> Self {
        Self::with_type(SseEventType::Error, error_message)
    }
}

impl fmt::Display for SseEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}

/// Helper to create SSE event from JSON-RPC streaming response
pub fn from_streaming_response(
    response: &SendStreamingMessageResponse,
) -> Result<SseEvent, serde_json::Error> {
    let data = serde_json::to_string(response)?;
    Ok(SseEvent::new(data))
}

/// Helper to create SSE event with event type from JSON-serializable data
pub fn create_event<T: Serialize>(
    event_type: SseEventType,
    data: &T,
) -> Result<SseEvent, serde_json::Error> {
    let data_str = serde_json::to_string(data)?;
    Ok(SseEvent::with_type(event_type, data_str))
}

/// SSE stream builder for easier construction
pub struct SseStreamBuilder {
    events: Vec<SseEvent>,
}

impl SseStreamBuilder {
    /// Create a new SSE stream builder
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Add an event to the stream
    pub fn add_event(mut self, event: SseEvent) -> Self {
        self.events.push(event);
        self
    }

    /// Add a data event
    pub fn add_data(self, data: impl Into<String>) -> Self {
        self.add_event(SseEvent::new(data))
    }

    /// Add a typed event with JSON data
    pub fn add_json_event<T: Serialize>(
        self,
        event_type: SseEventType,
        data: &T,
    ) -> Result<Self, serde_json::Error> {
        let event = create_event(event_type, data)?;
        Ok(self.add_event(event))
    }

    /// Add a heartbeat event
    pub fn add_heartbeat(self) -> Self {
        self.add_event(SseEvent::heartbeat())
    }

    /// Build the stream as a formatted string
    pub fn build(self) -> String {
        self.events.iter().map(|e| e.format()).collect::<String>()
    }

    /// Get the events
    pub fn events(self) -> Vec<SseEvent> {
        self.events
    }
}

impl Default for SseStreamBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse Last-Event-ID header for SSE resumption
pub fn parse_last_event_id(header_value: &str) -> Option<String> {
    if header_value.is_empty() {
        None
    } else {
        Some(header_value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_event_basic() {
        let event = SseEvent::new("test data");
        let formatted = event.format();
        assert!(formatted.contains("data: test data\n"));
        assert!(formatted.ends_with("\n\n"));
    }

    #[test]
    fn test_sse_event_with_type() {
        let event = SseEvent::with_type(SseEventType::TaskCreated, "test data");
        let formatted = event.format();
        assert!(formatted.contains("event: task-created\n"));
        assert!(formatted.contains("data: test data\n"));
    }

    #[test]
    fn test_sse_event_with_id() {
        let event = SseEvent::new("test data").with_id("123");
        let formatted = event.format();
        assert!(formatted.contains("id: 123\n"));
    }

    #[test]
    fn test_sse_event_with_retry() {
        let event = SseEvent::new("test data").with_retry(5000);
        let formatted = event.format();
        assert!(formatted.contains("retry: 5000\n"));
    }

    #[test]
    fn test_sse_event_with_comment() {
        let event = SseEvent::new("test data").with_comment("This is a comment");
        let formatted = event.format();
        assert!(formatted.contains(": This is a comment\n"));
    }

    #[test]
    fn test_sse_heartbeat() {
        let event = SseEvent::heartbeat();
        let formatted = event.format();
        assert!(formatted.contains("event: heartbeat\n"));
        assert!(formatted.contains("data: {}\n"));
    }

    #[test]
    fn test_sse_error() {
        let event = SseEvent::error("Something went wrong");
        let formatted = event.format();
        assert!(formatted.contains("event: error\n"));
        assert!(formatted.contains("data: Something went wrong\n"));
    }

    #[test]
    fn test_sse_stream_builder() {
        let stream = SseStreamBuilder::new()
            .add_data("first event")
            .add_heartbeat()
            .add_data("second event")
            .build();

        assert!(stream.contains("data: first event\n"));
        assert!(stream.contains("event: heartbeat\n"));
        assert!(stream.contains("data: second event\n"));
    }

    #[test]
    fn test_multiline_data() {
        let event = SseEvent::new("line1\nline2\nline3");
        let formatted = event.format();
        assert!(formatted.contains("data: line1\n"));
        assert!(formatted.contains("data: line2\n"));
        assert!(formatted.contains("data: line3\n"));
    }
}
