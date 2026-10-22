//! The call trace of one invocation: every `ctx.*` call it made as a timed
//! record, for the console's waterfall (`oxy_custom_apps::call_trace`).
//!
//! `GET /api/customer-apps/{id}/invocations/{invocation_id}/calls`, behind the
//! same fences as `/held` (`invocation_read`): the mount's staff guards, the
//! invocation belonging to the app in the path, and reach to its environment.
//! A recorded trace never changes. DB-only (FleetOk).

use axum::Json;
use axum::extract::Path;
use chrono::{DateTime, Duration, FixedOffset, Utc};
use entity::{app_function_invocation_calls, app_function_invocations};
use oxy_auth::extractor::AuthenticatedUserExtractor;
use serde::Serialize;
use uuid::Uuid;

use super::environment_scope::{self, ScopeError};
use super::invocation_read::{build_label, invocation_of, require_readable};
use crate::custom_apps_call_trace::{CallRecord, calls_of};
use crate::custom_apps_call_trace_store as store;

/// How long a `running` row may still be running: past the 300 s function
/// ceiling and the isolate's grace. Older, it is an orphan a crash left, and
/// reads as not recorded rather than being polled forever.
const RUNNING_WINDOW: Duration = Duration::seconds(330);

/// How long after a run settles its trace may still be on its way: the write
/// is spawned off the request path, so a read right after the answer can
/// precede it.
const WRITE_WINDOW: Duration = Duration::seconds(15);

/// One invocation's calls.
#[derive(Debug, Serialize)]
pub struct InvocationCalls {
    pub invocation_id: Uuid,
    pub function: String,
    pub environment: String,
    /// The publish's build id of the build that ran.
    pub build_id: Option<String>,
    pub mode: String,
    pub status: String,
    /// The pager's rule (`invocations::counted_as_failure`).
    pub failed: bool,
    pub duration_ms: Option<i64>,
    pub created_at: DateTime<FixedOffset>,
    /// A trace is stored for this invocation.
    pub recorded: bool,
    /// Why there is none: `running`, `expired`, `did_not_start` or
    /// `not_recorded`. `None` when recorded.
    pub reason: Option<&'static str>,
    pub init_ms: Option<i32>,
    pub total_calls: i32,
    pub dropped: i32,
    pub unfinished: i32,
    pub retention_days: i64,
    /// Sorted by start offset.
    pub calls: Vec<CallRecord>,
}

/// Why `invocation` has no readable trace at `now`, or `None` when `trace`
/// is one.
fn reason(
    invocation: &app_function_invocations::Model,
    trace: Option<&app_function_invocation_calls::Model>,
    retention_days: i64,
    now: DateTime<Utc>,
) -> Option<&'static str> {
    if let Some(trace) = trace {
        return trace.init_ms.is_none().then_some("did_not_start");
    }
    let started = invocation.created_at.with_timezone(&Utc);
    let settled = started + Duration::milliseconds(invocation.duration_ms.unwrap_or(0));
    Some(match invocation.status.as_str() {
        "shed" => "did_not_start",
        _ if now - started > Duration::days(retention_days) => "expired",
        "running" if now - started < RUNNING_WINDOW => "running",
        "running" => "not_recorded",
        _ if now - settled < WRITE_WINDOW => "running",
        _ => "not_recorded",
    })
}

/// `GET /admin/apps/{id}/invocations/{invocation_id}/calls` — the call trace
/// of one invocation of app `id`.
pub async fn get_invocation_calls(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    // Present iff authenticated via an app publish token, which reaches this
    // route on the `/customer-apps/{id}/…` mount.
    marker: Option<axum::Extension<oxy_auth::types::AppPublishTokenAuth>>,
    Path((id, invocation_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<InvocationCalls>, ScopeError> {
    let db = environment_scope::connect().await?;
    let app = environment_scope::load_app(&db, id).await?;
    let invocation = invocation_of(&db, id, invocation_id).await?;
    let caller = environment_scope::Caller::new(&user, marker.as_ref());
    require_readable(&db, &app, &caller, &invocation).await?;
    let trace = store::read(&db, id, invocation_id)
        .await
        .map_err(|e| ScopeError::internal("call trace lookup failed", e))?;
    let build_id = build_label(&db, id, invocation.build_id).await?;
    Ok(Json(answer(invocation, trace, build_id, Utc::now())))
}

fn answer(
    invocation: app_function_invocations::Model,
    trace: Option<app_function_invocation_calls::Model>,
    build_id: Option<String>,
    now: DateTime<Utc>,
) -> InvocationCalls {
    let retention_days = store::retention_days();
    let reason = reason(&invocation, trace.as_ref(), retention_days, now);
    let failed = super::invocations::counted_as_failure(
        &invocation.status,
        invocation.failure_fingerprint.as_deref(),
    );
    let (init_ms, total_calls, dropped, unfinished, calls) = match &trace {
        Some(t) => (
            t.init_ms,
            t.total_calls,
            t.dropped,
            t.unfinished,
            calls_of(&t.calls),
        ),
        None => (None, 0, 0, 0, Vec::new()),
    };
    InvocationCalls {
        invocation_id: invocation.id,
        function: invocation.function_name,
        environment: invocation.environment,
        build_id,
        mode: invocation.mode,
        status: invocation.status,
        failed,
        duration_ms: invocation.duration_ms,
        created_at: invocation.created_at,
        recorded: reason.is_none(),
        reason,
        init_ms,
        total_calls,
        dropped,
        unfinished,
        retention_days,
        calls,
    }
}

#[cfg(test)]
#[path = "invocation_calls_tests.rs"]
mod tests;
