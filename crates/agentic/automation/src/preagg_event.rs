//! Typed events emitted by the preagg background worker.
//!
//! Converts to the `(event_type, serde_json::Value)` wire format that
//! `agentic_runtime::worker::ExecutingTask::events` expects.
//!
//! # Serialize-only
//!
//! This type is `#[serde(untagged)]` so the wire payload is just the inner
//! fields (no `"type":` tag) — the discriminant lives in the parallel
//! `event_type` string column, not in the JSON body. As a consequence,
//! **`serde_json::from_value::<PreaggEvent>(payload)` is ambiguous**: the
//! `{view, rollup}` variants (`RollupFresh`, `RollupStarted`, `RollupDone`,
//! `RollupRetracted`, `RollupSkippedNoRefreshKey`) all share the same shape
//! and will always match the first variant. Consumers must deserialise using the
//! `event_type` column from the event_log, not by directly decoding into
//! `PreaggEvent`. If a future caller needs round-trip decoding, switch to
//! an internally-tagged representation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PreaggEvent {
    RollupFresh {
        view: String,
        rollup: String,
    },
    RollupStarted {
        view: String,
        rollup: String,
    },
    RollupDone {
        view: String,
        rollup: String,
    },
    /// The rebuild ran and the rollup is empty NOW, so its manifest entry and
    /// Parquet were removed rather than left serving the previous build's
    /// numbers under the Pre-aggregated badge. Distinct from `RollupDone`
    /// because the artifact is gone: a consumer waiting for a build timestamp
    /// to move would otherwise wait for one that will never arrive.
    RollupRetracted {
        view: String,
        rollup: String,
    },
    RollupFailed {
        view: String,
        rollup: String,
        error: String,
    },
    RefreshKeyError {
        rollup_hash: String,
        error: String,
    },
    RollupSkippedNoRefreshKey {
        view: String,
        rollup: String,
    },
    RollupSkippedNoDatasource {
        view: String,
        rollup: String,
        database: String,
    },
}

impl PreaggEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::RollupFresh { .. } => "preagg_rollup_fresh",
            Self::RollupStarted { .. } => "preagg_rollup_started",
            Self::RollupDone { .. } => "preagg_rollup_done",
            Self::RollupRetracted { .. } => "preagg_rollup_retracted",
            Self::RollupFailed { .. } => "preagg_rollup_failed",
            Self::RefreshKeyError { .. } => "preagg_refresh_key_error",
            Self::RollupSkippedNoRefreshKey { .. } => "preagg_rollup_skipped_no_refresh_key",
            Self::RollupSkippedNoDatasource { .. } => "preagg_rollup_skipped_no_datasource",
        }
    }

    /// Serialize to the wire format expected by `ExecutingTask::events`.
    pub fn to_wire(&self) -> (String, serde_json::Value) {
        let payload = serde_json::to_value(self).unwrap_or(serde_json::Value::Null);
        (self.event_type().to_string(), payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollup_done_event_type_string() {
        let ev = PreaggEvent::RollupDone {
            view: "sales".into(),
            rollup: "daily".into(),
        };
        assert_eq!(ev.event_type(), "preagg_rollup_done");
    }

    #[test]
    fn retracted_is_its_own_event_type() {
        // Same shape as RollupDone, deliberately not the same discriminant —
        // "rebuilt" and "removed" are what a reader has to tell apart.
        let ev = PreaggEvent::RollupRetracted {
            view: "sales".into(),
            rollup: "daily".into(),
        };
        assert_eq!(ev.event_type(), "preagg_rollup_retracted");
    }

    #[test]
    fn to_wire_payload_has_fields() {
        let ev = PreaggEvent::RollupFailed {
            view: "sales".into(),
            rollup: "daily".into(),
            error: "timeout".into(),
        };
        let (event_type, payload) = ev.to_wire();
        assert_eq!(event_type, "preagg_rollup_failed");
        assert_eq!(payload["error"], "timeout");
        // No duplicate "type" tag in the payload — the event_type string is the discriminant.
        assert!(payload.get("type").is_none());
    }

    #[test]
    fn refresh_key_error_wire_format() {
        let ev = PreaggEvent::RefreshKeyError {
            rollup_hash: "abc123".into(),
            error: "connector unavailable".into(),
        };
        let (event_type, payload) = ev.to_wire();
        assert_eq!(event_type, "preagg_refresh_key_error");
        assert_eq!(payload["rollup_hash"], "abc123");
    }

    #[test]
    fn skipped_no_datasource_wire_format() {
        let ev = PreaggEvent::RollupSkippedNoDatasource {
            view: "orders".into(),
            rollup: "orders_by_month".into(),
            database: "local".into(),
        };
        let (event_type, payload) = ev.to_wire();
        assert_eq!(event_type, "preagg_rollup_skipped_no_datasource");
        assert_eq!(payload["database"], "local");
        assert_eq!(payload["view"], "orders");
        assert!(payload.get("type").is_none());
    }
}
