//! Domain-specific events for the builder copilot pipeline.

use agentic_core::events::DomainEvents;
use serde::{Deserialize, Serialize};

/// Events emitted by the builder copilot.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum BuilderEvent {
    /// The LLM is proposing a file change and waiting for user confirmation.
    FileChangePending {
        /// Relative path (from project root) of the file to be changed.
        file_path: String,
        /// Human-readable description of the change.
        description: String,
        /// The new content that will be written if the user accepts.
        new_content: String,
        /// Current file content before the change; empty string for new files.
        #[serde(default)]
        old_content: String,
    },

    /// A file was actually written or deleted after the user accepted a proposal.
    FileChanged {
        /// Relative path (from project root) of the file that was changed.
        file_path: String,
        /// Human-readable description of what changed.
        description: String,
        /// Content of the file after the change; empty string for deletions.
        new_content: String,
        /// Content of the file before the change; empty string for new files.
        #[serde(default)]
        old_content: String,
        /// True when the file was deleted rather than written.
        #[serde(default)]
        is_deletion: bool,
    },

    /// A codebase tool was called (search/read).
    ToolUsed {
        /// Tool name (e.g. `"search_files"`, `"read_file"`, `"search_text"`).
        tool_name: String,
        /// One-line summary of what the tool did.
        summary: String,
    },
}

impl DomainEvents for BuilderEvent {}
