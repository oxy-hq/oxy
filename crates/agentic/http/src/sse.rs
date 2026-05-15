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
///
/// `done`/`error`/`cancelled` are the analytics/builder terminal events.
/// `subrun_completed` is the workflow domain's terminal event — without
/// it here, workflow SSE streams never close server-side: the loop keeps
/// waiting on the run's notifier even after the run row flips to `done`/
/// `failed`, the client's `fetchEventSource` promise never resolves, and
/// any `finally`-block cleanup (e.g. `setIsLoading(false)` in the chat
/// thread runner) never fires. The result is a spinner that hangs forever
/// after the workflow actually finished.
///
/// `source_type` is required because `subrun_completed` is the
/// *workflow run's own* terminal event for source_type="workflow", but
/// it's just an intermediate event for analytics/builder runs that
/// delegated to a procedure — the analytics pipeline still has more
/// events to emit once the child workflow returns. Without this gate,
/// the SSE loop would exit on the child procedure's `subrun_completed`
/// and the user would see the run frozen mid-stream until they
/// navigated away and back (which re-opens the SSE and replays the
/// post-resume events from the DB).
pub fn is_terminal(event_type: &str, source_type: &str) -> bool {
    matches!(event_type, "done" | "error" | "cancelled")
        || (source_type == "workflow" && event_type == "subrun_completed")
}

#[cfg(test)]
mod tests {
    use super::is_terminal;

    #[test]
    fn done_terminates_any_source() {
        assert!(is_terminal("done", "analytics"));
        assert!(is_terminal("done", "workflow"));
        assert!(is_terminal("done", "builder"));
    }

    /// Regression: a workflow delegated to from analytics emits
    /// `subrun_completed` mid-run. The analytics SSE used to exit
    /// there, freezing the UI until the user navigated away and back
    /// (which re-opens the stream and replays from the DB). Gate the
    /// terminal classification on `source_type == "workflow"` so only
    /// the procedure's own run treats it as terminal.
    #[test]
    fn subrun_completed_terminates_workflow_only() {
        assert!(is_terminal("subrun_completed", "workflow"));
        assert!(!is_terminal("subrun_completed", "analytics"));
        assert!(!is_terminal("subrun_completed", "builder"));
    }

    #[test]
    fn unrelated_events_are_not_terminal() {
        assert!(!is_terminal("subrun_started", "analytics"));
        assert!(!is_terminal("subrun_step_completed", "analytics"));
        assert!(!is_terminal("input_resolved", "analytics"));
        assert!(!is_terminal("input_resolved", "workflow"));
    }
}
