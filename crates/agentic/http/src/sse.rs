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
///
/// `load_completed`/`pipeline_error` are the airway domain's terminal
/// events (snake_case `AirwayEvent` tags). Airway never emits
/// `done`/`error`; a finished pipeline emits `load_completed` (even
/// when some resources were skipped → `completed_with_errors`) and a
/// failed one emits `pipeline_error`. Without gating these as terminal
/// for `source_type == "airway"`, the SSE loop never sets `terminal`
/// and only closes via the notifier-deregister fallback, which races
/// the final `load_completed` write — the client's stream stays open,
/// `streaming` stays true, and the run page spins forever even though
/// the data already finished loading. They're scoped to airway because
/// other domains have no such event types (no cross-domain risk), same
/// pattern as the workflow gate above.
///
/// `task_failed` is the coordinator's failure event when
/// `execute_airway` errors *before* the engine starts (secrets /
/// connector / destination / config resolution). The worker emits no
/// `pipeline_error` on that path, so for `source_type == "airway"`
/// `task_failed` is also terminal — otherwise the SSE only closes via
/// the notifier fallback and the run page shows nothing.
pub fn is_terminal(event_type: &str, source_type: &str) -> bool {
    matches!(event_type, "done" | "error" | "cancelled")
        || (source_type == "workflow" && event_type == "subrun_completed")
        || (source_type == "airway"
            && matches!(
                event_type,
                "load_completed" | "pipeline_error" | "task_failed"
            ))
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

    /// Regression: an airway run finishes by emitting `load_completed`
    /// (or `pipeline_error` on failure) — never `done`/`error`. Before
    /// gating these as terminal for `source_type == "airway"`, the SSE
    /// loop never closed deterministically and the run page spun
    /// forever after the load had actually finished (notably
    /// "completed with N skipped resource(s)" runs).
    #[test]
    fn airway_terminal_events_terminate_airway_only() {
        assert!(is_terminal("load_completed", "airway"));
        assert!(is_terminal("pipeline_error", "airway"));
        assert!(is_terminal("task_failed", "airway"));
        assert!(is_terminal("cancelled", "airway"));
        assert!(!is_terminal("load_completed", "analytics"));
        assert!(!is_terminal("pipeline_error", "workflow"));
        assert!(!is_terminal("task_failed", "analytics"));
        assert!(!is_terminal("load_progress", "airway"));
        assert!(!is_terminal("resource_failed", "airway"));
    }

    #[test]
    fn unrelated_events_are_not_terminal() {
        assert!(!is_terminal("subrun_started", "analytics"));
        assert!(!is_terminal("subrun_step_completed", "analytics"));
        assert!(!is_terminal("input_resolved", "analytics"));
        assert!(!is_terminal("input_resolved", "workflow"));
    }
}
