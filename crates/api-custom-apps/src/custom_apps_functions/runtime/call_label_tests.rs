//! What a call's trace record may say: label, facets and the trace's gate,
//! run together the way `host_call_task` runs them.

use std::time::Instant;

use super::super::call_note::CallNote;
use super::super::env_policy::{EnvPolicy, HostOp};
use super::super::host::FunctionCapabilities;
use super::super::host_error::HostError;
use super::call_label::CallLabel;
use super::host_call::{Facets, HostCall, HostCallKind, host_call_span, outcome_facets};
use super::test_calls::{SENTINEL, loaded};
use crate::custom_apps_call_trace::{CallRecord, CallStatus, CallTrace};

/// Label `call`, end it with `result`, and record it, as the broker does.
fn traced(
    call: &HostCall,
    note: CallNote,
    result: &Result<serde_json::Value, HostError>,
    labels: bool,
) -> CallRecord {
    let (_span, kind, op) = host_call_span(call);
    let label = CallLabel::of(call, op);
    let trace = CallTrace::new(Instant::now());
    let start = trace.begin(0);
    trace.record(
        start,
        label.into_facts(note, outcome_facets(kind, result, labels)),
    );
    trace.finish(1).calls.remove(0)
}

/// No value a call carries reaches its stored record, for any op, whether
/// the call succeeded with the value in its reply or failed with it in its
/// message.
#[test]
fn no_value_position_of_any_op_reaches_the_stored_trace() {
    let replies = [
        Ok(serde_json::json!({ "rows": [{ "email": SENTINEL }], "status": 200, "body": SENTINEL })),
        Err(HostError::backend(format!("boom: {SENTINEL} not found"))),
    ];
    for op in HostOp::ALL {
        for reply in &replies {
            let record = traced(&loaded(*op), CallNote::default(), reply, true);
            let document = serde_json::to_string(&record).unwrap();
            assert!(!document.contains(SENTINEL), "{op:?}: {document}");
        }
    }
}

#[test]
fn a_fetch_keeps_its_method_and_host_and_a_query_its_verb_and_table() {
    let (reply, _rx) = tokio::sync::oneshot::channel();
    let fetch = HostCall::Fetch {
        url: "https://u:p@api.x.com:8443/v1/orders?key=k".into(),
        init: serde_json::json!({ "method": "post" }),
        reply,
    };
    let ok = Ok(serde_json::json!({ "status": 201 }));
    let record = traced(&fetch, CallNote::default(), &ok, false);
    assert_eq!(record.target.as_deref(), Some("api.x.com"));
    assert_eq!(record.verb.as_deref(), Some("POST"));
    assert_eq!(record.http_status, Some(201));
    assert_eq!(record.status, CallStatus::Ok);

    let (reply, _rx) = tokio::sync::oneshot::channel();
    let query = HostCall::Query {
        sql: "select * from orders where email='a@b'".into(),
        reply,
    };
    let rows = Ok(serde_json::json!({ "rows": [{}, {}], "truncated": false }));
    let record = traced(&query, CallNote::default(), &rows, false);
    assert_eq!(
        (record.verb.as_deref(), record.target.as_deref()),
        (Some("SELECT"), Some("orders"))
    );
    assert_eq!((record.rows, record.rows_truncated), (Some(2), Some(false)));
}

/// The two positions the label reads as raw strings: a secret's name and a
/// warehouse database. Neither is an identifier here, so the trace's gate is
/// what keeps them out.
#[test]
fn a_raw_name_that_is_not_an_identifier_is_dropped_by_the_trace_gate() {
    let (reply, _rx) = tokio::sync::oneshot::channel();
    let secret = HostCall::SecretsSet {
        key: "a b@c".into(),
        value: "v".into(),
        reply,
    };
    let ok = Ok(serde_json::json!({}));
    assert_eq!(
        traced(&secret, CallNote::default(), &ok, false).target,
        None
    );

    let (reply, _rx) = tokio::sync::oneshot::channel();
    let warehouse = HostCall::WarehouseWrite {
        op: "insert".into(),
        payload: serde_json::json!({ "database": "x'y", "table": "t" }),
        reply,
    };
    assert_eq!(
        traced(&warehouse, CallNote::default(), &ok, false).target,
        None
    );
}

/// What the host resolved while running the op replaces the label's guess.
#[test]
fn the_host_note_names_the_namespace_and_table_of_a_database_op() {
    let (reply, _rx) = tokio::sync::oneshot::channel();
    let oltp = HostCall::Oltp {
        op: "exec".into(),
        payload: serde_json::json!({ "sql": "update orders set x = 1" }),
        reply,
    };
    let note = CallNote {
        namespace: Some("app_store".into()),
        verb: Some("UPDATE".into()),
        table: Some("orders".into()),
        held: false,
    };
    let record = traced(&oltp, note, &Ok(serde_json::json!({})), false);
    assert_eq!(record.op, "oltp.exec");
    assert_eq!(record.family, "oltp");
    assert_eq!(record.verb.as_deref(), Some("UPDATE"));
    assert_eq!(record.target.as_deref(), Some("app_store.orders"));
}

/// A gate refusal is an error of the fixed `not_allowed` kind; a call the
/// staging policy held is `held`, not an error; an upstream 5xx is an error.
#[test]
fn refusals_holds_and_upstream_failures_have_their_own_status() {
    let refusal = FunctionCapabilities::default()
        .refuse(HostOp::EmailSend)
        .unwrap_err();
    let facets = outcome_facets(HostCallKind::Other, &Err(refusal), false);
    assert_eq!(facets.status, CallStatus::Error);
    assert_eq!(facets.error_type, Some("not_allowed"));

    use oxy_app_core::custom_app_environment::AppEnvironment;
    let staging = EnvPolicy::for_environment(AppEnvironment::Staging);
    let held = staging.held_error(HostOp::Fetch);
    let facets = outcome_facets(HostCallKind::Fetch, &Err(held), true);
    assert_eq!(facets.status, CallStatus::Held);
    assert_eq!(facets.error_type, Some("held"));

    let facets = outcome_facets(
        HostCallKind::Fetch,
        &Ok(serde_json::json!({ "status": 503 })),
        false,
    );
    assert_eq!(
        facets,
        Facets {
            status: CallStatus::Error,
            error_type: Some("upstream_5xx"),
            http_status: Some(503),
            ..Facets::default()
        }
    );
}

/// A call the host held by answering for it — a staging fetch's 409 — is
/// held in the trace, not a success.
#[test]
fn a_call_the_host_answered_for_is_held() {
    let (reply, _rx) = tokio::sync::oneshot::channel();
    let fetch = HostCall::Fetch {
        url: "https://api.x.com/orders".into(),
        init: serde_json::json!({ "method": "POST" }),
        reply,
    };
    let note = CallNote {
        held: true,
        ..CallNote::default()
    };
    let record = traced(
        &fetch,
        note,
        &Ok(serde_json::json!({ "status": 409 })),
        true,
    );
    assert_eq!(record.status, CallStatus::Held);
    assert_eq!(record.error_type.as_deref(), Some("held"));
    assert_eq!(record.http_status, Some(409));
}
