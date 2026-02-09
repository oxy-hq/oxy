//! A2A protocol validation utilities
//!
//! This module provides validation functions for A2A protocol data structures
//! to ensure compliance with the specification.

use crate::error::{A2aError, A2aResult};
use crate::jsonrpc::JsonRpcRequest;
use crate::types::{AgentCard, Message, Part, Task, TaskState};

/// Validate a JSON-RPC request
pub fn validate_jsonrpc_request(request: &JsonRpcRequest) -> A2aResult<()> {
    // Validate JSON-RPC version
    if request.jsonrpc != "2.0" {
        return Err(A2aError::InvalidRequest(format!(
            "Invalid JSON-RPC version: {}. Expected '2.0'",
            request.jsonrpc
        )));
    }

    // Validate method name is not empty
    if request.method.is_empty() {
        return Err(A2aError::InvalidRequest(
            "Method name cannot be empty".to_string(),
        ));
    }

    // Validate method name format (should be category/action)
    if !request.method.contains('/') {
        return Err(A2aError::InvalidRequest(format!(
            "Invalid method name format: '{}'. Expected 'category/action'",
            request.method
        )));
    }

    Ok(())
}

/// Validate a message
pub fn validate_message(message: &Message) -> A2aResult<()> {
    // Validate message has at least one part
    if message.parts.is_empty() {
        return Err(A2aError::ValidationError(
            "Message must have at least one part".to_string(),
        ));
    }

    // Validate each part
    for part in &message.parts {
        validate_part(part)?;
    }

    // Validate message ID is not empty
    if message.message_id.is_empty() {
        return Err(A2aError::ValidationError(
            "Message ID cannot be empty".to_string(),
        ));
    }

    Ok(())
}

/// Validate a part
pub fn validate_part(part: &Part) -> A2aResult<()> {
    match part {
        Part::Text(text_part) => {
            if text_part.text.is_empty() {
                return Err(A2aError::ValidationError(
                    "Text part cannot be empty".to_string(),
                ));
            }
        }
        Part::File(file_part) => {
            // File parts should have either bytes or uri
            match &file_part.file {
                crate::types::FileContent::Bytes(bytes_file) => {
                    if bytes_file.bytes.is_empty() {
                        return Err(A2aError::ValidationError(
                            "File bytes cannot be empty".to_string(),
                        ));
                    }
                }
                crate::types::FileContent::Uri(uri_file) => {
                    if uri_file.uri.is_empty() {
                        return Err(A2aError::ValidationError(
                            "File URI cannot be empty".to_string(),
                        ));
                    }
                    // Validate URI format
                    if let Err(e) = url::Url::parse(&uri_file.uri) {
                        return Err(A2aError::ValidationError(format!(
                            "Invalid file URI: {}",
                            e
                        )));
                    }
                }
            }
        }
        Part::Data(_) => {
            // Data parts are always valid as long as they parse
        }
    }

    Ok(())
}

/// Validate a task
pub fn validate_task(task: &Task) -> A2aResult<()> {
    // Validate task ID is not empty
    if task.id.is_empty() {
        return Err(A2aError::ValidationError(
            "Task ID cannot be empty".to_string(),
        ));
    }

    // Validate context ID is not empty
    if task.context_id.is_empty() {
        return Err(A2aError::ValidationError(
            "Context ID cannot be empty".to_string(),
        ));
    }

    // Validate history messages if present
    if let Some(history) = &task.history {
        for message in history {
            validate_message(message)?;
        }
    }

    // Validate artifacts if present
    if let Some(artifacts) = &task.artifacts {
        for artifact in artifacts {
            // Validate artifact ID
            if artifact.artifact_id.is_empty() {
                return Err(A2aError::ValidationError(
                    "Artifact ID cannot be empty".to_string(),
                ));
            }

            // Validate artifact parts
            if artifact.parts.is_empty() {
                return Err(A2aError::ValidationError(
                    "Artifact must have at least one part".to_string(),
                ));
            }

            for part in &artifact.parts {
                validate_part(part)?;
            }
        }
    }

    Ok(())
}

/// Validate an agent card
pub fn validate_agent_card(card: &AgentCard) -> A2aResult<()> {
    // Validate protocol version
    if card.protocol_version.is_empty() {
        return Err(A2aError::ValidationError(
            "Protocol version cannot be empty".to_string(),
        ));
    }

    // Validate name
    if card.name.is_empty() {
        return Err(A2aError::ValidationError(
            "Agent name cannot be empty".to_string(),
        ));
    }

    // Validate description
    if card.description.is_empty() {
        return Err(A2aError::ValidationError(
            "Agent description cannot be empty".to_string(),
        ));
    }

    // Validate URL
    if card.url.is_empty() {
        return Err(A2aError::ValidationError(
            "Agent URL cannot be empty".to_string(),
        ));
    }

    // Validate URL format
    if let Err(e) = url::Url::parse(&card.url) {
        return Err(A2aError::ValidationError(format!(
            "Invalid agent URL: {}",
            e
        )));
    }

    // Validate version
    if card.version.is_empty() {
        return Err(A2aError::ValidationError(
            "Agent version cannot be empty".to_string(),
        ));
    }

    // Validate default input/output modes are not empty
    if card.default_input_modes.is_empty() {
        return Err(A2aError::ValidationError(
            "Default input modes cannot be empty".to_string(),
        ));
    }

    if card.default_output_modes.is_empty() {
        return Err(A2aError::ValidationError(
            "Default output modes cannot be empty".to_string(),
        ));
    }

    // Validate skills
    for skill in &card.skills {
        if skill.id.is_empty() {
            return Err(A2aError::ValidationError(
                "Skill ID cannot be empty".to_string(),
            ));
        }
        if skill.name.is_empty() {
            return Err(A2aError::ValidationError(
                "Skill name cannot be empty".to_string(),
            ));
        }
        if skill.description.is_empty() {
            return Err(A2aError::ValidationError(
                "Skill description cannot be empty".to_string(),
            ));
        }
    }

    // Validate additional interfaces if present
    if let Some(interfaces) = &card.additional_interfaces {
        for interface in interfaces {
            if interface.url.is_empty() {
                return Err(A2aError::ValidationError(
                    "Interface URL cannot be empty".to_string(),
                ));
            }
            // Validate URL format
            if let Err(e) = url::Url::parse(&interface.url) {
                return Err(A2aError::ValidationError(format!(
                    "Invalid interface URL: {}",
                    e
                )));
            }
        }
    }

    Ok(())
}

/// Validate task state transition
pub fn validate_task_state_transition(from: &TaskState, to: &TaskState) -> A2aResult<()> {
    use TaskState::*;

    let is_valid = match (from, to) {
        // From Submitted
        (Submitted, Working)
        | (Submitted, Rejected)
        | (Submitted, Canceled)
        | (Submitted, Failed)
        | (Submitted, AuthRequired) => true,

        // From Working
        (Working, Completed)
        | (Working, Failed)
        | (Working, Canceled)
        | (Working, InputRequired)
        | (Working, AuthRequired) => true,

        // From InputRequired
        (InputRequired, Working) | (InputRequired, Canceled) | (InputRequired, Failed) => true,

        // From AuthRequired
        (AuthRequired, Working) | (AuthRequired, Canceled) | (AuthRequired, Failed) => true,

        // Terminal states cannot transition
        (Completed, _) | (Canceled, _) | (Rejected, _) | (Failed, _) => false,

        // Same state is allowed (idempotent)
        (a, b) if a == b => true,

        // All other transitions are invalid
        _ => false,
    };

    if !is_valid {
        return Err(A2aError::ValidationError(format!(
            "Invalid task state transition from {:?} to {:?}",
            from, to
        )));
    }

    Ok(())
}

/// Check if a task state is terminal
pub fn is_terminal_state(state: &TaskState) -> bool {
    matches!(
        state,
        TaskState::Completed | TaskState::Canceled | TaskState::Rejected | TaskState::Failed
    )
}

/// Check if a task can be restarted
pub fn can_restart_task(state: &TaskState) -> bool {
    !is_terminal_state(state)
}

/// Validate MIME type format
pub fn validate_mime_type(mime_type: &str) -> A2aResult<()> {
    if mime_type.is_empty() {
        return Err(A2aError::ValidationError(
            "MIME type cannot be empty".to_string(),
        ));
    }

    // Basic validation: should have format type/subtype
    if !mime_type.contains('/') {
        return Err(A2aError::ValidationError(format!(
            "Invalid MIME type format: '{}'. Expected 'type/subtype'",
            mime_type
        )));
    }

    let parts: Vec<&str> = mime_type.split('/').collect();
    if parts.len() != 2 {
        return Err(A2aError::ValidationError(format!(
            "Invalid MIME type format: '{}'. Expected 'type/subtype'",
            mime_type
        )));
    }

    if parts[0].is_empty() || parts[1].is_empty() {
        return Err(A2aError::ValidationError(format!(
            "Invalid MIME type format: '{}'. Type and subtype cannot be empty",
            mime_type
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MessageKind, MessageRole, TextPart};

    #[test]
    fn test_validate_jsonrpc_request() {
        let valid_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "message/send".to_string(),
            params: None,
            id: Some(serde_json::json!(1)),
        };
        assert!(validate_jsonrpc_request(&valid_request).is_ok());

        let invalid_version = JsonRpcRequest {
            jsonrpc: "1.0".to_string(),
            method: "message/send".to_string(),
            params: None,
            id: Some(serde_json::json!(1)),
        };
        assert!(validate_jsonrpc_request(&invalid_version).is_err());

        let invalid_method = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "invalid-method".to_string(),
            params: None,
            id: Some(serde_json::json!(1)),
        };
        assert!(validate_jsonrpc_request(&invalid_method).is_err());
    }

    #[test]
    fn test_validate_message() {
        let valid_message = Message {
            kind: MessageKind::Message,
            role: MessageRole::User,
            parts: vec![Part::Text(TextPart::new("test"))],
            metadata: None,
            extensions: None,
            reference_task_ids: None,
            message_id: "test-id".to_string(),
            task_id: None,
            context_id: None,
        };
        assert!(validate_message(&valid_message).is_ok());

        let empty_parts = Message {
            parts: vec![],
            ..valid_message.clone()
        };
        assert!(validate_message(&empty_parts).is_err());
    }

    #[test]
    fn test_task_state_transitions() {
        assert!(validate_task_state_transition(&TaskState::Submitted, &TaskState::Working).is_ok());
        assert!(validate_task_state_transition(&TaskState::Working, &TaskState::Completed).is_ok());
        assert!(
            validate_task_state_transition(&TaskState::Completed, &TaskState::Working).is_err()
        );
        assert!(
            validate_task_state_transition(&TaskState::Working, &TaskState::Submitted).is_err()
        );
    }

    #[test]
    fn test_terminal_states() {
        assert!(is_terminal_state(&TaskState::Completed));
        assert!(is_terminal_state(&TaskState::Canceled));
        assert!(is_terminal_state(&TaskState::Failed));
        assert!(!is_terminal_state(&TaskState::Working));
        assert!(!is_terminal_state(&TaskState::Submitted));
    }

    #[test]
    fn test_validate_mime_type() {
        assert!(validate_mime_type("text/plain").is_ok());
        assert!(validate_mime_type("application/json").is_ok());
        assert!(validate_mime_type("invalid").is_err());
        assert!(validate_mime_type("").is_err());
        assert!(validate_mime_type("text/").is_err());
    }

    #[test]
    fn test_validate_part_text() {
        use crate::types::TextKind;

        let valid_text = Part::Text(TextPart::new("Hello world"));
        assert!(validate_part(&valid_text).is_ok());

        let empty_text = Part::Text(TextPart {
            kind: TextKind::Text,
            text: "".to_string(),
            metadata: None,
        });
        assert!(validate_part(&empty_text).is_err());
    }

    #[test]
    fn test_validate_part_file_with_uri() {
        use crate::types::{FileBase, FileContent, FileKind, FilePart, FileWithUri};

        let valid_file = Part::File(FilePart {
            kind: FileKind::File,
            file: FileContent::Uri(FileWithUri {
                base: FileBase {
                    name: None,
                    mime_type: Some("text/plain".to_string()),
                },
                uri: "https://example.com/file.txt".to_string(),
            }),
            metadata: None,
        });
        assert!(validate_part(&valid_file).is_ok());

        let empty_uri = Part::File(FilePart {
            kind: FileKind::File,
            file: FileContent::Uri(FileWithUri {
                base: FileBase {
                    name: None,
                    mime_type: None,
                },
                uri: "".to_string(),
            }),
            metadata: None,
        });
        assert!(validate_part(&empty_uri).is_err());

        let invalid_uri = Part::File(FilePart {
            kind: FileKind::File,
            file: FileContent::Uri(FileWithUri {
                base: FileBase {
                    name: None,
                    mime_type: None,
                },
                uri: "not-a-valid-uri".to_string(),
            }),
            metadata: None,
        });
        assert!(validate_part(&invalid_uri).is_err());
    }

    #[test]
    fn test_validate_part_file_with_bytes() {
        use crate::types::{FileBase, FileContent, FileKind, FilePart, FileWithBytes};

        let valid_bytes = Part::File(FilePart {
            kind: FileKind::File,
            file: FileContent::Bytes(FileWithBytes {
                base: FileBase {
                    name: None,
                    mime_type: Some("application/octet-stream".to_string()),
                },
                bytes: "AQIDBA==".to_string(), // Base64 for [1, 2, 3, 4]
            }),
            metadata: None,
        });
        assert!(validate_part(&valid_bytes).is_ok());

        let empty_bytes = Part::File(FilePart {
            kind: FileKind::File,
            file: FileContent::Bytes(FileWithBytes {
                base: FileBase {
                    name: None,
                    mime_type: None,
                },
                bytes: "".to_string(),
            }),
            metadata: None,
        });
        assert!(validate_part(&empty_bytes).is_err());
    }

    #[test]
    fn test_validate_task() {
        use crate::types::{TaskKind, TaskStatus};

        let valid_task = Task {
            id: "task-123".to_string(),
            context_id: "context-456".to_string(),
            status: TaskStatus::new(TaskState::Submitted),
            history: None,
            artifacts: None,
            metadata: None,
            kind: TaskKind::Task,
        };
        assert!(validate_task(&valid_task).is_ok());

        let empty_id = Task {
            id: "".to_string(),
            ..valid_task.clone()
        };
        assert!(validate_task(&empty_id).is_err());

        let empty_context = Task {
            context_id: "".to_string(),
            ..valid_task.clone()
        };
        assert!(validate_task(&empty_context).is_err());
    }

    #[test]
    fn test_validate_task_with_history() {
        use crate::types::{TaskKind, TaskStatus};

        let message = Message {
            kind: MessageKind::Message,
            role: MessageRole::User,
            parts: vec![Part::Text(TextPart::new("test"))],
            metadata: None,
            extensions: None,
            reference_task_ids: None,
            message_id: "msg-1".to_string(),
            task_id: None,
            context_id: None,
        };

        let task_with_history = Task {
            id: "task-123".to_string(),
            context_id: "context-456".to_string(),
            status: TaskStatus::new(TaskState::Working),
            history: Some(vec![message]),
            artifacts: None,
            metadata: None,
            kind: TaskKind::Task,
        };
        assert!(validate_task(&task_with_history).is_ok());

        let invalid_message = Message {
            parts: vec![],              // Empty parts - invalid
            message_id: "".to_string(), // Empty ID - invalid
            ..task_with_history.history.as_ref().unwrap()[0].clone()
        };

        let task_with_invalid_history = Task {
            history: Some(vec![invalid_message]),
            ..task_with_history.clone()
        };
        assert!(validate_task(&task_with_invalid_history).is_err());
    }

    #[test]
    fn test_validate_task_with_artifacts() {
        use crate::types::{Artifact, TaskKind, TaskStatus};

        let artifact = Artifact {
            artifact_id: "artifact-1".to_string(),
            name: Some("Result".to_string()),
            description: None,
            parts: vec![Part::Text(TextPart::new("Result"))],
            metadata: None,
            extensions: None,
        };

        let task_with_artifacts = Task {
            id: "task-123".to_string(),
            context_id: "context-456".to_string(),
            status: TaskStatus::new(TaskState::Completed),
            history: None,
            artifacts: Some(vec![artifact]),
            metadata: None,
            kind: TaskKind::Task,
        };
        assert!(validate_task(&task_with_artifacts).is_ok());

        let empty_artifact_id = Artifact {
            artifact_id: "".to_string(),
            ..task_with_artifacts.artifacts.as_ref().unwrap()[0].clone()
        };

        let task_with_invalid_artifact = Task {
            artifacts: Some(vec![empty_artifact_id]),
            ..task_with_artifacts.clone()
        };
        assert!(validate_task(&task_with_invalid_artifact).is_err());
    }

    #[test]
    fn test_validate_message_with_empty_id() {
        let message_with_empty_id = Message {
            kind: MessageKind::Message,
            role: MessageRole::User,
            parts: vec![Part::Text(TextPart::new("test"))],
            metadata: None,
            extensions: None,
            reference_task_ids: None,
            message_id: "".to_string(), // Empty ID
            task_id: None,
            context_id: None,
        };
        assert!(validate_message(&message_with_empty_id).is_err());
    }

    #[test]
    fn test_validate_jsonrpc_empty_method() {
        let empty_method = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "".to_string(),
            params: None,
            id: Some(serde_json::json!(1)),
        };
        assert!(validate_jsonrpc_request(&empty_method).is_err());
    }
}
