//! The call trace of one Oxy Function invocation: every `ctx.*` host call it
//! made, as a timed record, for the console's waterfall.
//!
//! **What a record holds is a subset of what that op's platform span (F2,
//! `custom_apps_functions::runtime::host_call_span`) already records, never
//! more**: the registry op and family, the start offset from the invocation's
//! zero, the duration, a status, the fixed `error.type` facet, and the
//! identifiers the span carries — a SQL verb and table, a fetch's host and
//! method, a namespace, a count. Never SQL text or parameters, a URL path,
//! query or userinfo, headers, bodies, email addresses, subjects, notify text,
//! storage keys, filters, secret values or a per-call error message: the
//! invocation row keeps the one message it failed with.
//!
//! The gate is applied here, in [`CallTrace::record`], so no caller can bypass
//! it: a `verb` or `target` that is not [`identifier_like`] is dropped, not
//! recorded.
//!
//! **Bounds.** At most [`MAX_TRACED_CALLS`] calls per invocation are kept,
//! decided when a call *begins* (by its arrival number at the broker), so the
//! kept set is the first calls to start — not the first to finish. The rest
//! are counted as `dropped`. A kept call still in flight when the invocation
//! settles (a fire-and-forget promise, a call detached by a timeout) is
//! counted as `unfinished`; it records nothing once the trace has closed.
//!
//! **The collector, and the alternative rejected.** A `tracing` layer over the
//! `oxy::host_call` spans would make "a subset of the span" true by
//! construction, with no label code. It was rejected because the layer would
//! have to be installed in every process and test harness that runs a
//! function, and wherever it was missing the waterfall would silently go
//! empty. This explicit collector rides `InvocationMeters`, which every run
//! already carries across the isolate boundary.
//!
//! Storage and retention are in [`crate::call_trace_store`].

use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::write_record::identifier_like;

/// Calls kept per invocation. The rest are counted, not kept.
pub const MAX_TRACED_CALLS: u32 = 200;

/// The version of the stored `{"v":…,"calls":[…]}` document.
pub const TRACE_VERSION: u64 = 1;

/// How one call ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CallStatus {
    #[default]
    Ok,
    /// Failed: the app's error, the platform's, or an upstream 5xx.
    Error,
    /// Held by a non-production environment's policy (it did not run).
    Held,
    /// Refused by the environment's policy.
    Refused,
}

/// One call's facts, from the broker: its registry op and the span's own
/// identifiers. Gated by [`CallTrace::record`].
#[derive(Clone, Debug, Default)]
pub struct CallFacts {
    pub op: &'static str,
    pub family: &'static str,
    pub status: CallStatus,
    pub error_type: Option<&'static str>,
    pub verb: Option<String>,
    pub target: Option<String>,
    pub count: Option<u32>,
    pub rows: Option<u64>,
    pub rows_truncated: Option<bool>,
    pub http_status: Option<u16>,
}

/// One kept call, as stored and served.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallRecord {
    /// Arrival order at the broker, from 0.
    pub seq: u32,
    pub op: String,
    pub family: String,
    /// Milliseconds from the invocation's zero (`InvocationMeters::start`).
    pub start_ms: u32,
    pub duration_ms: u32,
    pub status: CallStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verb: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows_truncated: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
}

/// A call the broker has begun: taken before dispatch, handed to
/// [`CallTrace::record`] when it returns.
#[derive(Clone, Copy, Debug)]
pub struct CallStart {
    seq: u32,
    at: Instant,
    start_ms: u32,
    kept: bool,
}

impl CallStart {
    pub fn seq(&self) -> u32 {
        self.seq
    }
}

/// The trace once the invocation has settled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSnapshot {
    /// Sorted by `(start_ms, seq)`.
    pub calls: Vec<CallRecord>,
    pub total_calls: u32,
    pub dropped: u32,
    pub unfinished: u32,
}

impl CallSnapshot {
    /// The stored document.
    pub fn document(&self) -> serde_json::Value {
        serde_json::json!({ "v": TRACE_VERSION, "calls": self.calls })
    }
}

/// The calls of a stored document. A version this binary does not know, or a
/// malformed document, reads as no calls rather than failing the read.
pub fn calls_of(document: &serde_json::Value) -> Vec<CallRecord> {
    if document.get("v").and_then(|v| v.as_u64()) != Some(TRACE_VERSION) {
        return Vec::new();
    }
    document
        .get("calls")
        .and_then(|c| serde_json::from_value(c.clone()).ok())
        .unwrap_or_default()
}

/// The per-invocation collector. Shared by every host-call task of one run.
#[derive(Debug)]
pub struct CallTrace {
    started: Instant,
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    calls: Vec<CallRecord>,
    closed: bool,
}

fn millis(d: std::time::Duration) -> u32 {
    d.as_millis().min(u32::MAX as u128) as u32
}

impl CallTrace {
    /// A trace whose zero is `started`, the invocation's.
    pub fn new(started: Instant) -> Self {
        Self {
            started,
            inner: Mutex::new(Inner::default()),
        }
    }

    /// Begin call number `seq` (its arrival order, from 0). Lock-free.
    pub fn begin(&self, seq: u32) -> CallStart {
        let at = Instant::now();
        CallStart {
            seq,
            at,
            start_ms: millis(at.saturating_duration_since(self.started)),
            kept: seq < MAX_TRACED_CALLS,
        }
    }

    /// Record a call that has returned. Dropped when it is past the cap or the
    /// trace has closed. The lock is held only for the push.
    pub fn record(&self, start: CallStart, facts: CallFacts) {
        if !start.kept {
            return;
        }
        let record = CallRecord {
            seq: start.seq,
            op: facts.op.to_string(),
            family: facts.family.to_string(),
            start_ms: start.start_ms,
            duration_ms: millis(start.at.elapsed()),
            status: facts.status,
            error_type: facts.error_type.map(str::to_string),
            verb: facts.verb.filter(|v| identifier_like(v)),
            target: facts.target.filter(|t| identifier_like(t)),
            count: facts.count,
            rows: facts.rows,
            rows_truncated: facts.rows_truncated,
            http_status: facts.http_status,
        };
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if !inner.closed {
            inner.calls.push(record);
        }
    }

    /// Close the trace and take what it holds. `total_calls` is every call the
    /// invocation made (`InvocationMeters::host_calls`).
    pub fn finish(&self, total_calls: u32) -> CallSnapshot {
        let mut calls = {
            let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            inner.closed = true;
            std::mem::take(&mut inner.calls)
        };
        calls.sort_by_key(|c| (c.start_ms, c.seq));
        let kept = total_calls.min(MAX_TRACED_CALLS);
        CallSnapshot {
            unfinished: kept.saturating_sub(calls.len() as u32),
            dropped: total_calls.saturating_sub(MAX_TRACED_CALLS),
            total_calls,
            calls,
        }
    }
}

#[cfg(test)]
#[path = "call_trace_tests.rs"]
mod tests;
