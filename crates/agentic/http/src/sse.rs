//! SSE event utilities.
//!
//! Domain-specific deserialization and UI transformation live in the
//! [`EventRegistry`](agentic_runtime::event_registry) in the runtime crate.
//! This module retains only:
//!
//! - [`UiEvent`] — serialized UI event type for REST responses.
//! - [`squash_deltas`] — merge consecutive token events for REST replay.
//! - [`is_terminal`] — check if an event type signals run termination.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A serialized UI event used in REST responses (e.g. `list_runs_by_thread`).
#[derive(Serialize, Deserialize)]
pub struct UiEvent {
    pub seq: i64,
    pub event_type: String,
    pub payload: Value,
    pub attempt: i32,
}

/// Squash consecutive delta events in a REST replay response.
///
/// Collapses consecutive runs of `text_delta` / `thinking_token` into a single
/// event with the concatenated token text.
pub fn squash_deltas(events: Vec<UiEvent>) -> Vec<UiEvent> {
    let mut out: Vec<UiEvent> = Vec::with_capacity(events.len());

    for ev in events {
        match ev.event_type.as_str() {
            "text_delta" | "thinking_token" => {
                let token = ev.payload["token"].as_str().unwrap_or("").to_string();
                if let Some(last) = out.last_mut()
                    && last.event_type == ev.event_type
                {
                    let merged = last.payload["token"].as_str().unwrap_or("").to_string() + &token;
                    last.payload = serde_json::json!({ "token": merged });
                    last.seq = ev.seq;
                    continue;
                }
                out.push(ev);
            }
            _ => out.push(ev),
        }
    }

    out
}

/// Returns true for event types that signal the run has terminated.
pub fn is_terminal(event_type: &str) -> bool {
    matches!(event_type, "done" | "error" | "cancelled")
}
