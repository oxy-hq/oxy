//! The bootstrap snapshot: an isolate that starts from it must be
//! indistinguishable from one that ran the bootstrap script itself.

use super::engine::deno::DenoEngine;
use super::engine::{ActiveEngine, IsolateConfig, IsolateEngine};
use super::{BOOTSTRAP_JS, heap_limit_bytes};

fn config() -> IsolateConfig {
    IsolateConfig {
        heap_limit: heap_limit_bytes(),
        bootstrap: BOOTSTRAP_JS,
    }
}

/// Evaluate `expr` in the isolate and return it JSON-stringified.
async fn eval(engine: &mut DenoEngine, expr: &str) -> String {
    let source = format!("(async () => JSON.stringify({expr}))()");
    let value = engine
        .invoke("oxy:test", source)
        .await
        .expect("the expression must evaluate");
    value.as_str().expect("a JSON string").to_owned()
}

/// Everything the bootstrap script leaves for a tenant to see: the global
/// scope, the shape of the `ctx` it builds, and the `Response` polyfill.
const OBSERVE: &str = r#"(() => {
    const walk = (o, p, out) => {
        for (const k of Object.keys(o).sort()) {
            const v = o[k];
            const q = p ? p + "." + k : k;
            out.push(q + ":" + typeof v);
            if (v && typeof v === "object") walk(v, q, out);
        }
        return out;
    };
    const ctx = globalThis.__buildCtx({
        user: { id: "u", orgId: "o", teams: [] },
        env: {},
        channel: "production",
    });
    const res = Response.json({ a: 1 }, { status: 201 });
    return {
        globals: Object.getOwnPropertyNames(globalThis).sort(),
        ctx: walk(ctx, "", []),
        response: [res.status, res.body, res.headers["content-type"]],
        legacyOrgId: ctx.user.org_id,
    };
})()"#;

/// Evaluate `expr` in an isolate made by `create`, then drop the isolate.
///
/// One at a time, on purpose: V8 tracks a "current" isolate per thread, and
/// `deno_core` panics on a runtime that is used while a newer one is alive.
async fn in_new_isolate(
    create: impl FnOnce(IsolateConfig) -> Result<DenoEngine, String>,
    expr: &str,
) -> String {
    let mut engine = create(config()).expect("isolate");
    eval(&mut engine, expr).await
}

#[tokio::test]
async fn an_isolate_started_from_the_snapshot_matches_one_that_ran_the_script() {
    let bare_view = in_new_isolate(DenoEngine::create_without_snapshot, OBSERVE).await;
    let warm_view = in_new_isolate(ActiveEngine::create, OBSERVE).await;

    assert!(
        bare_view.contains("\"__buildCtx\""),
        "the control isolate must have run the bootstrap, or the comparison proves nothing: {bare_view}"
    );
    assert_eq!(warm_view, bare_view);
}

/// V8 runs `--predictable --random-seed=42` for the whole process when the
/// first thing to touch it is a snapshot builder. Every tenant's
/// `Math.random()` would then be the same sequence in every process, and a
/// token a function derives from it guessable. Two isolates drawing the same
/// numbers is the symptom; this is run in its own process (nextest), where the
/// snapshot is built before any ordinary isolate exists.
#[tokio::test]
async fn building_the_snapshot_does_not_make_math_random_deterministic() {
    let draw = "Array.from({ length: 4 }, () => Math.random())";
    let first = in_new_isolate(ActiveEngine::create, draw).await;
    let second = in_new_isolate(ActiveEngine::create, draw).await;
    assert_ne!(
        first, second,
        "two isolates drew the same Math.random() sequence"
    );
}

/// Prints how long an isolate takes to become ready for the tenant's module,
/// with and without the snapshot. A measurement, not a budget: the only
/// assertions are on conditions (the snapshot exists; both paths produce a
/// working isolate), never on a duration.
#[tokio::test]
async fn isolate_creation_with_and_without_the_snapshot_is_measured() {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    const ROUNDS: usize = 30;

    let built = std::time::Instant::now();
    let first = ActiveEngine::create(config()).expect("first snapshot isolate");
    let first_ms = built.elapsed().as_secs_f64() * 1e3;
    drop(first);

    let median = |mut samples: Vec<f64>| {
        samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
        samples[samples.len() / 2]
    };
    let mut with_snapshot = Vec::new();
    let mut without_snapshot = Vec::new();
    for _ in 0..ROUNDS {
        let t = std::time::Instant::now();
        let mut engine = ActiveEngine::create(config()).expect("snapshot isolate");
        with_snapshot.push(t.elapsed().as_secs_f64() * 1e3);
        assert_eq!(
            eval(&mut engine, "typeof globalThis.__buildCtx").await,
            "\"function\""
        );
        drop(engine);

        let t = std::time::Instant::now();
        let mut engine = DenoEngine::create_without_snapshot(config()).expect("bare isolate");
        without_snapshot.push(t.elapsed().as_secs_f64() * 1e3);
        assert_eq!(
            eval(&mut engine, "typeof globalThis.__buildCtx").await,
            "\"function\""
        );
    }
    tracing::info!(
        rounds = ROUNDS,
        first_create_incl_snapshot_build_ms = first_ms,
        median_with_snapshot_ms = median(with_snapshot),
        median_without_snapshot_ms = median(without_snapshot),
        "isolate creation + bootstrap"
    );
}
