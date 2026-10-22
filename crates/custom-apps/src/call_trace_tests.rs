use std::time::{Duration, Instant};

use super::*;

fn facts(op: &'static str) -> CallFacts {
    CallFacts {
        op,
        family: "query",
        ..CallFacts::default()
    }
}

#[test]
fn the_cap_keeps_the_first_calls_to_start_and_counts_the_rest() {
    let trace = CallTrace::new(Instant::now());
    // Finished in reverse: the cap is decided at begin, not by who finishes first.
    let starts: Vec<CallStart> = (0..250).map(|seq| trace.begin(seq)).collect();
    for start in starts.into_iter().rev() {
        trace.record(start, facts("query"));
    }
    let snapshot = trace.finish(250);
    assert_eq!(snapshot.calls.len(), 200);
    assert_eq!(snapshot.dropped, 50);
    assert_eq!(snapshot.unfinished, 0);
    assert_eq!(snapshot.total_calls, 250);
    assert!(snapshot.calls.iter().all(|c| c.seq < 200));
}

#[test]
fn a_call_recorded_after_finish_is_unfinished_not_kept() {
    let trace = CallTrace::new(Instant::now());
    let first = trace.begin(0);
    let late = trace.begin(1);
    trace.record(first, facts("query"));
    let snapshot = trace.finish(2);
    trace.record(late, facts("fetch"));
    assert_eq!(snapshot.calls.len(), 1);
    assert_eq!(snapshot.unfinished, 1);
    assert_eq!(snapshot.dropped, 0);
    assert!(trace.finish(2).calls.is_empty(), "the late call stayed out");
}

#[test]
fn the_snapshot_is_sorted_by_start_then_seq() {
    let zero = Instant::now() - Duration::from_millis(50);
    let trace = CallTrace::new(zero);
    let a = trace.begin(0);
    std::thread::sleep(Duration::from_millis(5));
    let b = trace.begin(1);
    trace.record(b, facts("fetch"));
    trace.record(a, facts("query"));
    let calls = trace.finish(2).calls;
    assert_eq!(calls.iter().map(|c| c.seq).collect::<Vec<_>>(), [0, 1]);
    assert!(
        calls[0].start_ms >= 50,
        "offset is from the invocation's zero"
    );
    assert!(calls[1].start_ms > calls[0].start_ms);
}

/// The guard for the gate: a value that is not identifier-shaped is dropped
/// from `verb` and `target`, whatever the caller handed in.
#[test]
fn the_gate_drops_a_verb_or_target_that_is_not_an_identifier() {
    let rejected = [
        "orders where id = 1",
        "o'brien",
        "a@b.com",
        "api.x.com/v1/orders",
        "api.x.com?key=k",
        "\"quoted\"",
        &"t".repeat(65),
    ];
    for value in rejected {
        let trace = CallTrace::new(Instant::now());
        let start = trace.begin(0);
        trace.record(
            start,
            CallFacts {
                verb: Some(value.to_string()),
                target: Some(value.to_string()),
                ..facts("query")
            },
        );
        let call = &trace.finish(1).calls[0];
        assert_eq!(call.verb, None, "{value}");
        assert_eq!(call.target, None, "{value}");
    }
    let trace = CallTrace::new(Instant::now());
    let start = trace.begin(0);
    trace.record(
        start,
        CallFacts {
            verb: Some("SELECT".into()),
            target: Some("app_store.orders".into()),
            ..facts("query")
        },
    );
    let call = &trace.finish(1).calls[0];
    assert_eq!(call.verb.as_deref(), Some("SELECT"));
    assert_eq!(call.target.as_deref(), Some("app_store.orders"));
}

#[test]
fn the_document_round_trips_at_version_one_and_omits_absent_facts() {
    let trace = CallTrace::new(Instant::now());
    let start = trace.begin(0);
    trace.record(
        start,
        CallFacts {
            op: "fetch",
            family: "fetch",
            status: CallStatus::Error,
            error_type: Some("timeout"),
            verb: Some("POST".into()),
            target: Some("api.x.com".into()),
            ..CallFacts::default()
        },
    );
    let snapshot = trace.finish(1);
    let document = snapshot.document();
    assert_eq!(document["v"], 1);
    let call = &document["calls"][0];
    assert_eq!(call["status"], "error");
    assert!(
        call.get("rows").is_none(),
        "absent facts are omitted: {call}"
    );
    assert_eq!(calls_of(&document), snapshot.calls);
    assert!(calls_of(&serde_json::json!({ "v": 2, "calls": [] })).is_empty());
    assert!(calls_of(&serde_json::json!("not a document")).is_empty());
}
