//! Integration utilities for connecting A2A protocol with Oxy's ChatService.
//!
//! This module provides conversion functions and utilities to bridge the gap
//! between the A2A protocol and Oxy's existing chat service infrastructure.
//! It enables A2A handlers to leverage ChatService for:
//!
//! - Thread and message management
//! - Agent execution with proper context
//! - Streaming responses with artifact tracking
//! - Tool execution and result handling
//! - File output management

use a2a::{
    error::A2aError,
    types::{Artifact, Message, Part, Task, TaskState, TaskStatus, TextPart},
};
use std::collections::HashMap;

use crate::{api::agent::AskAgentResponse, service::chat::ChatExecutionRequest};
use oxy::{adapters::session_filters::SessionFilters, config::model::ConnectionOverrides};

/// Request wrapper that adapts an A2A message to ChatExecutionRequest interface.
///
/// This struct allows A2A messages to be executed through ChatService by
/// implementing the ChatExecutionRequest trait.
pub struct A2aMessageRequest {
    /// The prompt extracted from the A2A message
    pub question: String,
    /// Optional filters for session-based filtering
    pub filters: Option<SessionFilters>,
    /// Optional connection overrides for database connections
    pub connections: Option<ConnectionOverrides>,
    /// Optional global variables for execution context
    pub globals: Option<indexmap::IndexMap<String, serde_json::Value>>,
}

impl A2aMessageRequest {
    /// Create a new A2A message request from an A2A Message.
    ///
    /// This extracts the prompt from the message parts and converts any
    /// metadata into ChatService-compatible format.
    pub fn from_a2a_message(message: &Message) -> Result<Self, A2aError> {
        // Convert message parts to prompt string
        let question = super::mapper::a2a_message_to_prompt(message)?;

        // Extract filters, connections, and globals from message metadata if present
        let filters = message
            .metadata
            .as_ref()
            .and_then(|m| m.get("filters"))
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        let connections = message
            .metadata
            .as_ref()
            .and_then(|m| m.get("connections"))
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        let globals = message
            .metadata
            .as_ref()
            .and_then(|m| m.get("globals"))
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        Ok(Self {
            question,
            filters,
            connections,
            globals,
        })
    }
}

impl ChatExecutionRequest for A2aMessageRequest {
    fn get_question(&self) -> Option<String> {
        Some(self.question.clone())
    }

    fn get_filters(&self) -> Option<SessionFilters> {
        self.filters.clone()
    }

    fn get_connections(&self) -> Option<ConnectionOverrides> {
        self.connections.clone()
    }

    fn get_globals(&self) -> Option<indexmap::IndexMap<String, serde_json::Value>> {
        self.globals.clone()
    }
}

/// Convert an AskAgentResponse from ChatService to an A2A Task with artifacts.
///
/// This function maps the ChatService response structure to A2A format:
/// - Text content → TextPart
/// - Artifacts → Individual A2A Artifacts with appropriate parts
/// - Tool results → DataPart
/// - File outputs → FilePart
/// - Errors → Failed TaskStatus
///
/// # Arguments
///
/// * `response` - The AskAgentResponse from ChatService
/// * `task_id` - The A2A task ID
/// * `context_id` - The A2A context ID for grouping
/// * `original_message` - The original A2A message for history
///
/// # Returns
///
/// A completed A2A Task with all artifacts mapped from the chat response.
pub fn chat_response_to_task(
    response: AskAgentResponse,
    task_id: &str,
    context_id: &str,
    original_message: &Message,
) -> Result<Task, A2aError> {
    let mut artifacts = Vec::new();

    // Convert main content to artifact if present
    if !response.content.is_empty() {
        let text_part = Part::Text(TextPart::new(response.content.clone()));
        let artifact = Artifact::new(vec![text_part]).with_description("Agent response");
        artifacts.push(artifact);
    }

    // Convert ChatService artifacts to A2A artifacts
    for chat_artifact in &response.artifacts {
        let artifact = convert_chat_artifact_to_a2a(chat_artifact)?;
        artifacts.push(artifact);
    }

    // Determine task state based on response success
    let state = if response.success {
        TaskState::Completed
    } else {
        TaskState::Failed
    };

    // Create status message
    let status_text = if let Some(error) = &response.error_message {
        format!("Task failed: {}", error)
    } else {
        format!(
            "Task completed successfully with {} artifact(s)",
            artifacts.len()
        )
    };

    let status = TaskStatus::new(state).with_message(Message::new_agent(vec![Part::Text(
        TextPart::new(status_text),
    )]));
    // Timestamp is automatically set in TaskStatus::new()

    // Create task with artifacts
    Ok(Task {
        id: task_id.to_string(),
        context_id: context_id.to_string(),
        status,
        history: Some(vec![original_message.clone()]),
        artifacts: if artifacts.is_empty() {
            None
        } else {
            Some(artifacts)
        },
        metadata: Some({
            let mut map = HashMap::new();
            if let Some(usage) = &response.usage
                && let Ok(usage_value) = serde_json::to_value(usage)
            {
                map.insert("usage".to_string(), usage_value);
            }
            if !response.references.is_empty()
                && let Ok(refs_value) = serde_json::to_value(&response.references)
            {
                map.insert("references".to_string(), refs_value);
            }
            map
        }),
        kind: a2a::types::TaskKind::Task,
    })
}

/// Convert a ChatService artifact to an A2A Artifact.
///
/// Maps artifact types:
/// - SQL artifacts → DataPart with SQL content
/// - Chart/visualization artifacts → DataPart with JSON
/// - File artifacts → FilePart
/// - Other types → TextPart
fn convert_chat_artifact_to_a2a(
    chat_artifact: &crate::api::agent::ArtifactInfo,
) -> Result<Artifact, A2aError> {
    let part = match chat_artifact.kind.as_str() {
        "sql" | "query" => {
            // SQL artifacts become DataPart
            Part::Data(a2a::types::DataPart {
                kind: a2a::types::DataKind::Data,
                data: serde_json::Value::String(chat_artifact.id.clone()),
                metadata: None,
            })
        }
        "chart" | "visualization" => {
            // Chart artifacts become DataPart with JSON
            Part::Data(a2a::types::DataPart {
                kind: a2a::types::DataKind::Data,
                data: serde_json::Value::String(chat_artifact.id.clone()),
                metadata: None,
            })
        }
        "file" => {
            // File artifacts become FilePart with bytes
            Part::File(a2a::types::FilePart {
                kind: a2a::types::FileKind::File,
                file: a2a::types::FileContent::Bytes(a2a::types::FileWithBytes {
                    base: a2a::types::FileBase {
                        name: Some(chat_artifact.title.clone()),
                        mime_type: Some("application/octet-stream".to_string()),
                    },
                    bytes: String::new(), // Empty bytes for now
                }),
                metadata: None,
            })
        }
        _ => {
            // Default to text part
            Part::Text(TextPart::new(format!(
                "{}: {}",
                chat_artifact.kind, chat_artifact.title
            )))
        }
    };

    Ok(Artifact::new(vec![part])
        .with_name(&chat_artifact.title)
        .with_description(&chat_artifact.title))
}
