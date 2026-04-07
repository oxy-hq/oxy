//! Domain-specific events for the builder copilot pipeline.

use agentic_core::events::DomainEvents;
use serde::{Deserialize, Serialize};

/// Events emitted by the builder copilot.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum BuilderEvent {
    /// The LLM is proposing a file change and waiting for user confirmation.
    ProposedChange {
        /// Relative path (from project root) of the file to be changed.
        file_path: String,
        /// Human-readable description of the change.
        description: String,
        /// The new content that will be written if the user accepts.
        new_content: String,
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
