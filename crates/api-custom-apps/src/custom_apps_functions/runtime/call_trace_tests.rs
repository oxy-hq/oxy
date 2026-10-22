//! The call trace a real isolate run leaves, against `tests.rs`'s host.

use std::time::Duration;

use oxy_custom_apps::call_trace::{CallSnapshot, CallStatus};

use super::*;

/// Run `artifact` on `host` and hand back the response and the settled trace.
async fn traced_run(
    artifact: &str,
    host: Arc<MockHost>,
) -> (Result<FnResponse, RuntimeError>, CallSnapshot, Option<u32>) {
    let meters = InvocationMeters::start();
    let (_cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    let result = run(
        artifact.to_string(),
        uuid::Uuid::new_v4(),
        test_ctx(),
        FnRequest::from_body(b"{}".to_vec()),
        host,
        cancel_rx,
        Duration::from_secs(10),
        Arc::new(std::sync::Mutex::new(Vec::new())),
        meters.clone(),
        tracing::Span::none(),
    )
    .await;
    let snapshot = meters.calls.finish(meters.host_calls());
    (result, snapshot, meters.init_ms())
}

fn slow_host(delay_ms: u64) -> Arc<MockHost> {
    Arc::new(MockHost {
        query_delay: Some(Duration::from_millis(delay_ms)),
        ..Default::default()
    })
}

/// Two awaited-together queries overlap in the trace, start after the
/// isolate did, and the call awaited after them starts after they end.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_calls_overlap_and_a_sequential_one_follows_them() {
    let artifact = "export default async (req, ctx) => {
        await Promise.all([ctx.query('select 1 from a'), ctx.query('select 2 from b')]);
        await ctx.query('select 3 from c');
        return Response.json({ ok: true });
    };";
    let (result, snapshot, init_ms) = traced_run(artifact, slow_host(50)).await;
    result.unwrap();
    assert_eq!(
        (snapshot.total_calls, snapshot.dropped, snapshot.unfinished),
        (3, 0, 0)
    );
    let [a, b, c] = snapshot.calls.as_slice() else {
        panic!("three calls: {:?}", snapshot.calls);
    };
    let init_ms = init_ms.expect("tenant code ran");
    for call in [a, b, c] {
        assert_eq!((call.op.as_str(), call.status), ("query", CallStatus::Ok));
        assert!(call.start_ms >= init_ms, "{call:?} before init {init_ms}");
        assert!(call.duration_ms >= 50, "{call:?}");
        assert_eq!(call.rows, Some(1));
    }
    let end = |call: &oxy_custom_apps::call_trace::CallRecord| call.start_ms + call.duration_ms;
    assert!(b.start_ms < end(a), "a and b overlap: {a:?} {b:?}");
    assert!(c.start_ms >= end(a).max(end(b)) - 1, "c follows: {c:?}");
    assert_eq!(
        [&a.target, &b.target, &c.target].map(|t| t.as_deref()),
        [Some("a"), Some("b"), Some("c")]
    );
}

/// A call the function never awaited, still in flight when it returned, is
/// counted as unfinished rather than kept.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_call_still_running_when_the_invocation_ends_is_unfinished() {
    let artifact = "export default async (req, ctx) => {
        ctx.query('select 1 from slow');
        return Response.json({ ok: true });
    };";
    let (result, snapshot, _) = traced_run(artifact, slow_host(1_500)).await;
    result.unwrap();
    assert_eq!(snapshot.total_calls, 1);
    assert_eq!(snapshot.unfinished, 1, "{snapshot:?}");
    assert!(snapshot.calls.is_empty());
}

/// The broker's gates are traced too: a refused capability is a `not_allowed`
/// error that took no time at the host, a staging refusal is `refused`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gate_refusal_is_recorded_with_its_kind() {
    let host = Arc::new(MockHost {
        caps: Some(FunctionCapabilities::default()),
        ..Default::default()
    });
    let call = r#"ctx.email.send({ to: "a@b.com", subject: "s", text: "t" })"#;
    let (_, snapshot, _) = traced_run(&attempt(call), host).await;
    let [refused] = snapshot.calls.as_slice() else {
        panic!("one call: {snapshot:?}");
    };
    assert_eq!(refused.op, "email.send");
    assert_eq!(refused.status, CallStatus::Error);
    assert_eq!(refused.error_type.as_deref(), Some("not_allowed"));
    assert_eq!(refused.count, Some(1));
    assert!(refused.duration_ms < 50, "{refused:?}");

    use oxy_app_core::custom_app_environment::AppEnvironment;
    let staging = Arc::new(MockHost {
        policy: Some(EnvPolicy::for_environment(AppEnvironment::Staging)),
        ..Default::default()
    });
    let (_, snapshot, _) = traced_run(&attempt(r#"ctx.airway.run("orders_sync")"#), staging).await;
    let [airway] = snapshot.calls.as_slice() else {
        panic!("one call: {snapshot:?}");
    };
    assert_eq!(airway.status, CallStatus::Refused);
    assert_eq!(airway.target.as_deref(), Some("orders_sync"));
}
