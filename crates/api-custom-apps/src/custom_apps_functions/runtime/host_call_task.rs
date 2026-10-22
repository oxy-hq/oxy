//! One `ctx.*` call, serviced on a task of its own so concurrent awaits in
//! one function do not serialize: dispatch, record the outcome on the op's
//! span, note a failure (or a recovery) for the pager, add the call to the
//! invocation's trace, then answer the isolate.
//!
//! The trace record is added **before** the reply, so an awaited call is in
//! the trace before the isolate can resume and finish. A call still running
//! when the invocation settles finds the trace closed and is counted as
//! unfinished (`oxy_custom_apps::call_trace`).

use std::sync::Arc;

use sentry::SentryFutureExt;
use tracing::Instrument;

use super::FunctionHost;
use super::call_label::CallLabel;
use super::host_call::{
    HostCall, dispatch_host_call, host_call_span, host_call_target, outcome_facets,
    record_host_call_outcome,
};
use crate::custom_apps_telemetry::InvocationMeters;

/// Count `call`, begin its trace record, and service it on a spawned task
/// under its own span — a child of the invocation span the broker is polled
/// in (`tokio::spawn` carries no span by itself) — and the invocation's
/// Sentry hub.
pub(super) fn spawn_host_call(
    call: HostCall,
    host: Arc<dyn FunctionHost>,
    meters: &InvocationMeters,
) {
    // Counted here, where every `ctx.*` call funnels through, so the count
    // cannot drift as ops are added; the count is the call's arrival number.
    let start = meters.begin_call();
    let trace = meters.calls.clone();
    let (span, kind, op) = host_call_span(&call);
    // Read before dispatch consumes the call: the failure note keys a later
    // success to the same target, and the trace labels the call by it.
    let target = host_call_target(&call);
    let label = CallLabel::of(&call, op);
    tokio::spawn(
        async move {
            let host_note = host.clone();
            let ((reply, result), note) =
                super::super::call_note::scoped(dispatch_host_call(call, host)).await;
            let labels = host_note.decides_by_environment();
            let facets = outcome_facets(kind, &result, labels);
            record_host_call_outcome(&facets);
            // Before the reply: once the handler can catch the error, the
            // failure is already noted.
            match &result {
                Err(error) => {
                    let error_kind =
                        super::super::host_error_class::classify_host_error_typed(error, labels);
                    if super::super::host_call_attrs::counts_toward_paging(error_kind) {
                        host_note.note_host_call_failure(
                            op,
                            error_kind,
                            &error.message,
                            target.as_deref(),
                        );
                    }
                }
                Ok(_) => host_note.note_host_call_success(op, target.as_deref()),
            }
            trace.record(start, label.into_facts(note, facets));
            let _ = reply.send(result);
        }
        .instrument(span)
        .bind_hub(sentry::Hub::current()),
    );
}
