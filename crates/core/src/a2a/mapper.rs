//! Bidirectional conversion between A2A and Oxy formats.
//!
//! This module provides comprehensive mapping functions for converting between
//! A2A protocol types and Oxy internal types, including:
//!
//! - **Message Conversion**: A2A Messages ↔ Oxy prompts
//! - **Artifact Generation**: Oxy agent responses → A2A Artifacts
//! - **Streaming Conversion**: Oxy streaming events → A2A SSE events
//! - **Error Mapping**: Oxy errors → A2A TaskStatus failures
//!
//! # Architecture
//!
//! The mapper operates in two directions:
//!
//! 1. **A2A → Oxy**: Convert incoming A2A requests to Oxy execution format
//!    - `a2a_message_to_prompt()` - Extract prompt text from A2A Message
//!    - `a2a_parts_to_content()` - Convert A2A Parts to Oxy content
//!
//! 2. **Oxy → A2A**: Convert Oxy responses to A2A protocol format
//!    - `oxy_response_to_artifacts()` - Main conversion function
//!    - `oxy_output_to_parts()` - Convert OutputContainer to Parts
//!    - `create_artifact()` - Group parts into artifacts
//!    - `generate_artifact_metadata()` - Create IDs and descriptions
//!
//! # Examples
//!
//! ## Converting A2A Message to Oxy Format
//!
//! ```rust,ignore
//! use a2a::types::{Message, Part, TextPart};
//! use oxy_core::a2a::mapper::a2a_message_to_prompt;
//!
//! let message = Message::new_user(vec![
//!     Part::Text(TextPart::new("What's the weather today?")),
//! ]);
//!
//! let prompt = a2a_message_to_prompt(&message)?;
//! assert_eq!(prompt, "What's the weather today?");
//! ```
//!
//! ## Converting Oxy Response to A2A Artifacts
//!
//! ```rust,ignore
//! use oxy_core::a2a::mapper::oxy_response_to_artifacts;
//! use oxy_core::execute::types::{Output, OutputContainer};
//!
//! let output = OutputContainer::Single(Output::Text("Hello, world!".to_string()));
//! let artifacts = oxy_response_to_artifacts(&output)?;
//!
//! assert_eq!(artifacts.len(), 1);
//! assert_eq!(artifacts[0].parts.len(), 1);
//! ```

use a2a::{
    error::A2aError,
    types::{Artifact, DataPart, FileContent, Message, Part, TextPart},
};

use crate::execute::types::{Output, OutputContainer, Table};

/// Convert A2A Message to a prompt string for Oxy agent execution.
///
/// This function converts all parts of an A2A message into a formatted prompt string
/// that can be used by Oxy agents. It handles:
///
/// - **Text Parts**: Concatenated directly
/// - **File Parts**: Described with filename and MIME type, with note about data availability
/// - **Data Parts**: Pretty-printed JSON representation with MIME type
///
/// # Arguments
///
/// * `message` - The A2A message to convert
///
/// # Returns
///
/// A prompt string suitable for passing to an Oxy agent.
///
/// # Errors
///
/// - `InvalidParams` if the message contains no parts
/// - `InvalidParams` if JSON parsing fails for data parts
///
/// # Examples
///
/// ```rust,ignore
/// let message = Message::new_user(vec![
///     Part::Text(TextPart::new("Analyze this data:")),
///     Part::Data(DataPart::new(json!({"key": "value"}))),
/// ]);
///
/// let prompt = a2a_message_to_prompt(&message)?;
/// // Result includes both text and formatted JSON data
/// ```
pub fn a2a_message_to_prompt(message: &Message) -> Result<String, A2aError> {
    if message.parts.is_empty() {
        return Err(A2aError::InvalidParams(
            "Message must contain at least one part".to_string(),
        ));
    }

    let mut prompt_parts = Vec::new();

    for part in &message.parts {
        match part {
            Part::Text(text_part) => {
                // Add text content directly
                prompt_parts.push(text_part.text.clone());
            }
            Part::File(file_part) => {
                // For file parts, create a descriptive text representation
                // In the future, we could enhance this to actually download and process files
                let file_name = match &file_part.file {
                    FileContent::Bytes(f) => f.base.name.as_deref().unwrap_or("unnamed"),
                    FileContent::Uri(f) => f.base.name.as_deref().unwrap_or("unnamed"),
                };

                let mut file_desc = format!("[File: {}]", file_name);

                // Add MIME type if available
                let mime_type = match &file_part.file {
                    FileContent::Bytes(f) => &f.base.mime_type,
                    FileContent::Uri(f) => &f.base.mime_type,
                };
                if let Some(mt) = mime_type {
                    file_desc.push_str(&format!(" ({})", mt));
                }

                // Add source information
                match &file_part.file {
                    FileContent::Bytes(_) => {
                        file_desc.push_str(" [Data available inline]");
                    }
                    FileContent::Uri(f) => {
                        file_desc.push_str(&format!(" [URL: {}]", f.uri));
                    }
                }

                prompt_parts.push(file_desc);
            }
            Part::Data(data_part) => {
                // For data parts, format the JSON in a readable way
                let mut data_desc = String::new();

                // Check metadata for MIME type if present
                if let Some(metadata) = &data_part.metadata {
                    if let Some(mime) = metadata.get("mimeType").and_then(|v| v.as_str()) {
                        data_desc.push_str(&format!("[Data: {}]\n", mime));
                    } else {
                        data_desc.push_str("[Data]\n");
                    }
                } else {
                    data_desc.push_str("[Data]\n");
                }

                // Pretty-print the JSON data
                let formatted_json = serde_json::to_string_pretty(&data_part.data)
                    .map_err(|e| A2aError::InvalidParams(format!("Invalid JSON data: {}", e)))?;

                data_desc.push_str(&formatted_json);
                prompt_parts.push(data_desc);
            }
        }
    }

    Ok(prompt_parts.join("\n\n"))
}

/// Convert Oxy OutputContainer to A2A Artifacts.
///
/// This is the main conversion function that transforms Oxy agent execution
/// results into A2A Artifact format. It handles various output types:
///
/// - **Text**: Converted to TextPart
/// - **Tables**: Converted to DataPart with JSON representation
/// - **Maps**: Each entry becomes a separate artifact
/// - **Lists**: Each item becomes a separate artifact
/// - **Metadata/Consistency**: Unwrapped and converted recursively
///
/// # Arguments
///
/// * `output` - The Oxy OutputContainer from agent execution
///
/// # Returns
///
/// A vector of Artifacts representing the agent's output. May contain multiple
/// artifacts if the output is structured (map/list) or contains multiple types.
///
/// # Examples
///
/// ```rust,ignore
/// let output = OutputContainer::Single(Output::Text("Result".to_string()));
/// let artifacts = oxy_response_to_artifacts(&output)?;
/// assert_eq!(artifacts.len(), 1);
/// ```
pub fn oxy_response_to_artifacts(output: &OutputContainer) -> Result<Vec<Artifact>, A2aError> {
    match output {
        OutputContainer::Single(single_output) => {
            // Convert single output to parts
            let parts = oxy_output_to_parts(single_output)?;
            Ok(vec![create_artifact(
                parts,
                Some("Agent response".to_string()),
            )])
        }
        OutputContainer::List(list) => {
            // Each list item becomes a separate artifact
            let mut artifacts = Vec::new();
            for (index, item) in list.iter().enumerate() {
                let item_artifacts = oxy_response_to_artifacts(item)?;
                for artifact in item_artifacts {
                    artifacts.push(artifact.with_description(format!("Item {}", index + 1)));
                }
            }
            Ok(artifacts)
        }
        OutputContainer::Map(map) => {
            // Each map entry becomes a separate artifact
            let mut artifacts = Vec::new();
            for (key, value) in map.iter() {
                // Skip variables (they're internal metadata)
                if matches!(value, OutputContainer::Variable(_)) {
                    continue;
                }

                let value_artifacts = oxy_response_to_artifacts(value)?;
                for artifact in value_artifacts {
                    artifacts.push(artifact.with_description(key.clone()));
                }
            }

            // If we have no artifacts (all were variables), create a single artifact with the map
            if artifacts.is_empty() {
                let json_value = output.to_json().map_err(|e| {
                    A2aError::ServerError(format!("Failed to convert to JSON: {}", e))
                })?;
                let parts = vec![Part::Data(DataPart::new(json_value))];
                return Ok(vec![create_artifact(
                    parts,
                    Some("Structured output".to_string()),
                )]);
            }

            Ok(artifacts)
        }
        OutputContainer::Variable(value) => {
            // Variables are JSON values - convert to DataPart
            let parts = vec![Part::Data(DataPart::new(value.clone()))];
            Ok(vec![create_artifact(
                parts,
                Some("Variable value".to_string()),
            )])
        }
        OutputContainer::Metadata { value } => {
            // Unwrap metadata and convert the inner value
            oxy_response_to_artifacts(&value.output)
        }
        OutputContainer::Consistency { value, score } => {
            // Unwrap consistency wrapper and add score to description
            let mut artifacts = oxy_response_to_artifacts(&value.output)?;
            for artifact in artifacts.iter_mut() {
                let desc = artifact
                    .description
                    .clone()
                    .unwrap_or_else(|| "Result".to_string());
                artifact.description = Some(format!("{} (consistency score: {:.2})", desc, score));
            }
            Ok(artifacts)
        }
    }
}

/// Convert a single Oxy Output to A2A Parts.
///
/// This function handles the conversion of atomic Oxy output types to their
/// corresponding A2A Part representations.
///
/// # Arguments
///
/// * `output` - The Oxy Output to convert
///
/// # Returns
///
/// A vector of Parts representing the output. Most outputs produce a single part,
/// but complex types like Tables may produce multiple parts.
///
/// # Supported Output Types
///
/// - **Text/Prompt/SQL**: Converted to TextPart
/// - **Bool**: Converted to TextPart with "true" or "false"
/// - **Table**: Converted to DataPart with JSON representation
/// - **Documents**: Converted to TextPart with document content
/// - **OmniQuery**: Converted to DataPart with query parameters
pub fn oxy_output_to_parts(output: &Output) -> Result<Vec<Part>, A2aError> {
    match output {
        Output::Text(text) => Ok(vec![Part::Text(TextPart::new(text.clone()))]),
        Output::Prompt(_prompt) => Ok(vec![]),
        Output::SQL(sql) => Ok(vec![Part::Text(TextPart::new(format!(
            "```sql\n{}\n```",
            sql.0
        )))]),
        Output::Bool(b) => Ok(vec![Part::Text(TextPart::new(b.to_string()))]),
        Output::Table(table) => {
            // Convert table to JSON DataPart
            let json_map = table.to_json().map_err(|e| {
                A2aError::ServerError(format!("Failed to convert table to JSON: {}", e))
            })?;
            Ok(vec![Part::Data(DataPart::new(serde_json::Value::Object(
                json_map,
            )))])
        }
        Output::Documents(docs) => {
            // Convert documents to text parts
            let text = docs
                .iter()
                .map(|doc| {
                    format!(
                        "Document ID: {}\nType: {}\n{}",
                        doc.id, doc.kind, doc.content
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            Ok(vec![Part::Text(TextPart::new(text))])
        }
        Output::OmniQuery(query) => {
            // Convert OmniQuery to JSON DataPart
            let json_value = serde_json::to_value(query).map_err(|e| {
                A2aError::ServerError(format!("Failed to serialize OmniQuery: {}", e))
            })?;
            Ok(vec![Part::Data(DataPart::new(json_value))])
        }
    }
}

/// Create an Artifact from a list of Parts.
///
/// This function generates a new Artifact with optional description.
/// It groups related parts together into a coherent artifact.
///
/// # Arguments
///
/// * `parts` - The parts to include in the artifact
/// * `description` - Optional description for the artifact
///
/// # Returns
///
/// A new Artifact
fn create_artifact(parts: Vec<Part>, description: Option<String>) -> Artifact {
    let mut artifact = Artifact::new(parts);

    if let Some(desc) = description {
        artifact = artifact.with_description(desc);
    }

    artifact
}

/// Convert Oxy error to A2A TaskStatus with failed state.
///
/// This function creates a TaskStatus representing a failed task, including
/// the error message and optionally the error details.
///
/// # Arguments
///
/// * `error` - The Oxy error that caused the failure
///
/// # Returns
///
/// A TaskStatus with TaskState::Failed and error message
pub fn oxy_error_to_task_status(error: &crate::errors::OxyError) -> a2a::types::TaskStatus {
    let error_message = error.to_string();

    a2a::types::TaskStatus::new(a2a::types::TaskState::Failed).with_message(
        a2a::types::Message::new_agent(vec![Part::Text(TextPart::new(format!(
            "Task failed: {}",
            error_message
        )))]),
    )
}

/// Convert Oxy Table to A2A DataPart.
///
/// This is a helper function for converting table data to structured JSON format
/// suitable for A2A DataPart.
///
/// # Arguments
///
/// * `table` - The Oxy Table to convert
///
/// # Returns
///
/// A DataPart containing the table data in JSON format
pub fn oxy_table_to_data_part(table: &Table) -> Result<DataPart, A2aError> {
    let json_map = table
        .to_json()
        .map_err(|e| A2aError::ServerError(format!("Failed to convert table to JSON: {}", e)))?;

    Ok(DataPart::new(serde_json::Value::Object(json_map)))
}

/// Convert tool execution result to A2A DataPart.
///
/// This function takes a tool result (typically JSON) and wraps it in a DataPart.
/// This is useful for exposing tool execution results as structured data.
///
/// # Arguments
///
/// * `result` - The tool result as a JSON value
///
/// # Returns
///
/// A DataPart containing the tool result
pub fn oxy_tool_result_to_data_part(result: serde_json::Value) -> DataPart {
    DataPart::new(result)
}

// ============================================================================
// Streaming Conversion Functions
// ============================================================================

/// Convert Oxy streaming chunk to A2A Part.
///
/// This function handles real-time conversion of streaming output chunks from
/// Oxy agent execution to A2A Parts suitable for SSE streaming.
///
/// # Arguments
///
/// * `chunk` - The streaming chunk text
///
/// # Returns
///
/// A Part representing the chunk (typically TextPart)
///
/// # Note
///
/// This is a simplified implementation. In the future, this could handle:
/// - Incremental table updates
/// - Partial file streaming
/// - Tool call progress updates
pub fn oxy_stream_chunk_to_part(chunk: &str) -> Part {
    Part::Text(TextPart::new(chunk.to_string()))
}

/// Create an artifact.created SSE event.
///
/// This function generates an SSE event for the `artifact.created` event type,
/// which is sent when a new artifact is generated during streaming.
///
/// # Arguments
///
/// * `artifact` - The artifact that was created
///
/// # Returns
///
/// An SseEvent for artifact creation
///
/// # Note
///
/// The actual SSE formatting is handled by the `a2a` crate's streaming utilities.
pub fn create_artifact_created_event(artifact: &Artifact) -> a2a::streaming::SseEvent {
    let data = serde_json::to_string(artifact).unwrap_or_default();
    a2a::streaming::SseEvent::with_type(a2a::streaming::SseEventType::ArtifactUpdate, data)
}

/// Create a task.completed SSE event.
///
/// This function generates an SSE event for task completion during streaming.
///
/// # Arguments
///
/// * `task_id` - The ID of the completed task
/// * `artifacts` - Final list of all artifacts generated
///
/// # Returns
///
/// An SseEvent for task completion
pub fn create_task_completed_event(
    task_id: &str,
    artifacts: &[Artifact],
) -> a2a::streaming::SseEvent {
    let event_data = serde_json::json!({
        "task_id": task_id,
        "artifacts": artifacts,
    });

    let data = serde_json::to_string(&event_data).unwrap_or_default();
    a2a::streaming::SseEvent::with_type(a2a::streaming::SseEventType::TaskCompleted, data)
}

/// Create a task.failed SSE event.
///
/// This function generates an SSE event for task failure during streaming.
///
/// # Arguments
///
/// * `task_id` - The ID of the failed task
/// * `error` - The error that caused the failure
///
/// # Returns
///
/// An SseEvent for task failure
pub fn create_task_failed_event(
    task_id: &str,
    error: &crate::errors::OxyError,
) -> a2a::streaming::SseEvent {
    let event_data = serde_json::json!({
        "task_id": task_id,
        "error": error.to_string(),
    });

    let data = serde_json::to_string(&event_data).unwrap_or_default();
    a2a::streaming::SseEvent::with_type(a2a::streaming::SseEventType::TaskFailed, data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2a::types::{Message, Part, TextPart};

    #[test]
    fn test_a2a_message_to_prompt_single_text() {
        let message = Message::new_user(vec![Part::Text(TextPart::new("Hello"))]);

        let result = a2a_message_to_prompt(&message).unwrap();
        assert_eq!(result, "Hello");
    }

    #[test]
    fn test_a2a_message_to_prompt_multiple_texts() {
        let message = Message::new_user(vec![
            Part::Text(TextPart::new("Hello")),
            Part::Text(TextPart::new("World")),
        ]);

        let result = a2a_message_to_prompt(&message).unwrap();
        assert_eq!(result, "Hello\n\nWorld");
    }

    #[test]
    fn test_a2a_message_to_prompt_no_parts() {
        let message = Message::new_user(vec![]);

        let result = a2a_message_to_prompt(&message);
        assert!(result.is_err());
    }

    #[test]
    fn test_a2a_message_to_prompt_with_file() {
        use a2a::types::{FileBase, FileContent, FileKind, FilePart, FileWithBytes};

        let file_part = FilePart {
            kind: FileKind::File,
            file: FileContent::Bytes(FileWithBytes {
                base: FileBase {
                    name: Some("data.csv".to_string()),
                    mime_type: Some("text/csv".to_string()),
                },
                bytes: "Y29sMSxjb2wyDTEsMg==".to_string(), // base64 encoded "col1,col2\n1,2"
            }),
            metadata: None,
        };

        let message = Message::new_user(vec![
            Part::Text(TextPart::new("Analyze this file:")),
            Part::File(file_part),
        ]);

        let result = a2a_message_to_prompt(&message).unwrap();
        assert!(result.contains("Analyze this file:"));
        assert!(result.contains("[File: data.csv]"));
        assert!(result.contains("(text/csv)"));
        assert!(result.contains("[Data available inline]"));
    }

    #[test]
    fn test_a2a_message_to_prompt_with_file_url() {
        use a2a::types::{FileBase, FileContent, FileKind, FilePart, FileWithUri};

        let file_part = FilePart {
            kind: FileKind::File,
            file: FileContent::Uri(FileWithUri {
                base: FileBase {
                    name: Some("document.pdf".to_string()),
                    mime_type: Some("application/pdf".to_string()),
                },
                uri: "https://example.com/doc.pdf".to_string(),
            }),
            metadata: None,
        };

        let message = Message::new_user(vec![Part::File(file_part)]);

        let result = a2a_message_to_prompt(&message).unwrap();
        assert!(result.contains("[File: document.pdf]"));
        assert!(result.contains("(application/pdf)"));
        assert!(result.contains("[URL: https://example.com/doc.pdf]"));
    }

    #[test]
    fn test_a2a_message_to_prompt_with_data() {
        use a2a::types::{DataKind, DataPart};
        use serde_json::json;
        use std::collections::HashMap;

        let mut metadata = HashMap::new();
        metadata.insert("mimeType".to_string(), json!("application/json"));

        let data_part = DataPart {
            kind: DataKind::Data,
            data: json!({
                "name": "John",
                "age": 30,
                "active": true
            }),
            metadata: Some(metadata),
        };

        let message = Message::new_user(vec![
            Part::Text(TextPart::new("Process this data:")),
            Part::Data(data_part),
        ]);

        let result = a2a_message_to_prompt(&message).unwrap();
        assert!(result.contains("Process this data:"));
        assert!(result.contains("[Data: application/json]"));
        assert!(result.contains("\"name\""));
        assert!(result.contains("\"John\""));
        assert!(result.contains("\"age\""));
        assert!(result.contains("30"));
    }

    #[test]
    fn test_a2a_message_to_prompt_mixed_parts() {
        use a2a::types::{
            DataKind, DataPart, FileBase, FileContent, FileKind, FilePart, FileWithBytes,
        };
        use serde_json::json;
        use std::collections::HashMap;

        let mut metadata = HashMap::new();
        metadata.insert("mimeType".to_string(), json!("application/json"));

        let message = Message::new_user(vec![
            Part::Text(TextPart::new("Analyze this:")),
            Part::File(FilePart {
                kind: FileKind::File,
                file: FileContent::Bytes(FileWithBytes {
                    base: FileBase {
                        name: Some("data.json".to_string()),
                        mime_type: Some("application/json".to_string()),
                    },
                    bytes: "eyJrZXkiOiAidmFsdWUifQ==".to_string(), // base64 encoded JSON
                }),
                metadata: None,
            }),
            Part::Data(DataPart {
                kind: DataKind::Data,
                data: json!({"count": 42}),
                metadata: Some(metadata),
            }),
            Part::Text(TextPart::new("What insights do you see?")),
        ]);

        let result = a2a_message_to_prompt(&message).unwrap();

        // Check that all parts are present
        assert!(result.contains("Analyze this:"));
        assert!(result.contains("[File: data.json]"));
        assert!(result.contains("[Data: application/json]"));
        assert!(result.contains("\"count\""));
        assert!(result.contains("What insights do you see?"));

        // Check separation between parts
        assert!(result.contains("\n\n"));
    }

    #[test]
    fn test_oxy_response_to_artifacts_text() {
        let output = OutputContainer::Single(Output::Text("Hello, world!".to_string()));
        let artifacts = oxy_response_to_artifacts(&output).unwrap();

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].parts.len(), 1);

        match &artifacts[0].parts[0] {
            Part::Text(text_part) => {
                assert_eq!(text_part.text, "Hello, world!");
            }
            _ => panic!("Expected TextPart"),
        }
    }

    #[test]
    fn test_oxy_response_to_artifacts_bool() {
        let output = OutputContainer::Single(Output::Bool(true));
        let artifacts = oxy_response_to_artifacts(&output).unwrap();

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].parts.len(), 1);

        match &artifacts[0].parts[0] {
            Part::Text(text_part) => {
                assert_eq!(text_part.text, "true");
            }
            _ => panic!("Expected TextPart"),
        }
    }

    #[test]
    fn test_oxy_stream_chunk_to_part() {
        let part = oxy_stream_chunk_to_part("streaming text");

        match part {
            Part::Text(text_part) => {
                assert_eq!(text_part.text, "streaming text");
            }
            _ => panic!("Expected TextPart"),
        }
    }

    #[test]
    fn test_oxy_tool_result_to_data_part() {
        let result = serde_json::json!({
            "status": "success",
            "data": [1, 2, 3]
        });

        let data_part = oxy_tool_result_to_data_part(result.clone());
        assert_eq!(data_part.data, result);
    }
}
