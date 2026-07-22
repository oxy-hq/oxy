//! Tests for the stuck-run sweeper.
//!
//! The sweeper is defense-in-depth: even after `commit_decision` made the
//! decision boundary atomic, an automation run can still be stranded if the
//! coordinator crashes between receiving a `Suspended` outcome and calling
//! `insert_child_run` + `transport.assign`. In that window the parent's
//! queue row has already been released as `completed`, so the reaper cannot
//! rescue it — its grace is keyed on `queue_status='claimed'`.
//!
//! The sweeper finds any automation run that is still in a non-terminal
//! `task_status` but has no queue entry in `queued`/`claimed` (for itself or
//! any descendant task_id), and re-enqueues a fresh `AutomationDecision`. The
//! decider is idempotent under the `decision_version` CAS, so a spurious
//! re-enqueue is safe.
//!
//! Run:
//!   cargo nextest run -p agentic-runtime --test stuck_run_sweeper_test

use std::time::Duration;

use agentic_core::delegation::TaskSpec;
use agentic_runtime::crud;
use agentic_runtime::migration::RuntimeMigrator;
use agentic_runtime::transport::DurableTransport;
use sea_orm::{Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;

static TEST_DB_URL: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
static TEST_CONTAINER: tokio::sync::OnceCell<
    std::sync::Arc<testcontainers::ContainerAsync<testcontainers_modules::postgres::Postgres>>,
> = tokio::sync::OnceCell::const_new();

async fn test_db() -> Option<DatabaseConnection> {
    let url = TEST_DB_URL
        .get_or_init(|| async {
            if let Ok(url) = std::env::var("OXY_DATABASE_URL") {
                return url;
            }
            use testcontainers::runners::AsyncRunner;
            use testcontainers::{ImageExt, ReuseDirective};
            use testcontainers_modules::postgres::Postgres;
            let container = TEST_CONTAINER
                .get_or_init(|| async {
                    std::sync::Arc::new(
                        Postgres::default()
                            .with_tag("18-alpine")
                            .with_reuse(ReuseDirective::Always)
                            .start()
                            .await
                            .expect("failed to start Postgres testcontainer"),
                    )
                })
                .await;
            let port = container.get_host_port_ipv4(5432_u16).await.unwrap();
            format!("postgresql://postgres:postgres@127.0.0.1:{port}/postgres")
        })
        .await
        .clone();

    let mut db = None;
    for attempt in 0..10 {
        match Database::connect(&url).await {
            Ok(conn) => {
                db = Some(conn);
                break;
            }
            Err(e) if attempt < 9 => {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                eprintln!("test_db: attempt {attempt} failed: {e}");
            }
            Err(e) => panic!("failed to connect after 10 retries: {e}"),
        }
    }
    let db = db.unwrap();
    RuntimeMigrator::up(&db, None)
        .await
        .expect("runtime migrations failed");
    Some(db)
}

/// Shift `agentic_runs.updated_at` for `run_id` back by `secs` seconds so the
/// sweeper's grace check treats the run as old enough to act on.
async fn age_run(db: &DatabaseConnection, run_id: &str, secs: i64) {
    use sea_orm::{ConnectionTrait, Statement};
    db.execute(Statement::from_sql_and_values(
        sea_orm::DatabaseBackend::Postgres,
        "UPDATE agentic_runs SET updated_at = updated_at - ($1 || ' seconds')::interval WHERE id = $2",
        [secs.into(), run_id.into()],
    ))
    .await
    .unwrap();
}

async fn seed_automation_run(db: &DatabaseConnection) -> String {
    let run_id = format!("wf-stuck-{}", uuid::Uuid::new_v4());
    crud::insert_run(db, &run_id, "Q", None, "workflow", None, uuid::Uuid::nil())
        .await
        .unwrap();
    run_id
}

/// An automation run in `running` state with no queue row (or any descendant
/// queue row) is stranded; the sweeper should surface it.
#[tokio::test(flavor = "multi_thread")]
async fn find_stuck_runs_detects_run_with_no_queue_entry() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };

    let run_id = seed_automation_run(&db).await;
    age_run(&db, &run_id, 60).await;

    let stuck = crud::find_stuck_automation_runs(&db, 30).await.unwrap();
    assert!(
        stuck.iter().any(|r| r.run_id == run_id),
        "expected {run_id} in stuck runs, got: {stuck:?}"
    );
}

/// An automation run whose child task is still `claimed` is NOT stuck — the
/// child will drive the parent forward when it finishes. The sweeper must
/// not false-positive here or it would spawn duplicate decisions.
#[tokio::test(flavor = "multi_thread")]
async fn find_stuck_runs_ignores_run_with_in_flight_child() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };

    let run_id = seed_automation_run(&db).await;
    age_run(&db, &run_id, 60).await;

    // Child task claimed and heart-beating: parent is healthy, not stuck.
    let child_id = format!("{run_id}.1");
    crud::enqueue_task(
        &db,
        &child_id,
        &run_id,
        Some(&run_id),
        &TaskSpec::Agent {
            agent_id: "a".into(),
            question: "q".into(),
            extra: None,
        },
        None,
        crud::TaskScope::Global,
    )
    .await
    .unwrap();
    crud::claim_task(&db, "worker-x").await.unwrap();

    let stuck = crud::find_stuck_automation_runs(&db, 30).await.unwrap();
    assert!(
        !stuck.iter().any(|r| r.run_id == run_id),
        "run with in-flight child must not be reported stuck"
    );
}

/// Recently-updated automations (inside the grace window) are skipped — they
/// may be mid-commit from another worker. Acting on them would race.
#[tokio::test(flavor = "multi_thread")]
async fn find_stuck_runs_respects_grace_window() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };

    let run_id = seed_automation_run(&db).await;
    // No age_run — run was just created; updated_at is near now().

    let stuck = crud::find_stuck_automation_runs(&db, 30).await.unwrap();
    assert!(
        !stuck.iter().any(|r| r.run_id == run_id),
        "recently-updated run must be excluded by grace window"
    );
}

/// Terminal runs (`done`, `failed`, `cancelled`) are not the sweeper's job.
#[tokio::test(flavor = "multi_thread")]
async fn find_stuck_runs_ignores_terminal_runs() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };

    let run_id = seed_automation_run(&db).await;
    age_run(&db, &run_id, 60).await;
    crud::update_run_done(&db, &run_id, "done", None)
        .await
        .unwrap();
    age_run(&db, &run_id, 60).await; // ensure still outside grace after update

    let stuck = crud::find_stuck_automation_runs(&db, 30).await.unwrap();
    assert!(!stuck.iter().any(|r| r.run_id == run_id));
}

/// End-to-end: `run_stuck_run_sweeper` on a `DurableTransport` re-enqueues a
/// `AutomationDecision` for each stuck run. The queue row is upsert-safe, so a
/// second sweep must not re-rescue the same run.
///
/// The test DB is shared across tests via a reused testcontainer, so the
/// sweep may also rescue runs seeded by other tests. The assertion is
/// scoped to this test's own run to stay isolated.
#[tokio::test(flavor = "multi_thread")]
async fn sweeper_re_enqueues_automation_decision_idempotently() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };
    let run_id = seed_automation_run(&db).await;
    age_run(&db, &run_id, 60).await;

    let transport = DurableTransport::with_config(db.clone(), Duration::from_millis(100));

    // No queue row before sweeping.
    assert!(crud::get_queue_entry(&db, &run_id).await.unwrap().is_none());

    let rescued = transport.run_stuck_run_sweeper(30).await;
    assert!(rescued >= 1, "expected to rescue at least this run");

    // Queue row now present with an AutomationDecision spec for this run.
    let entry = crud::get_queue_entry(&db, &run_id).await.unwrap().unwrap();
    assert_eq!(entry.queue_status, "queued");
    let spec: TaskSpec = serde_json::from_value(entry.spec).unwrap();
    assert!(
        matches!(spec, TaskSpec::AutomationDecision { .. }),
        "expected AutomationDecision spec"
    );

    // Idempotent for OUR run: a second sweep may rescue other tests' stuck
    // runs, but our run already has a `queued` entry and must not be
    // re-rescued (queue status stays `queued`, not `claimed` since no
    // worker is running here).
    let _ = transport.run_stuck_run_sweeper(30).await;
    let entry_after = crud::get_queue_entry(&db, &run_id).await.unwrap().unwrap();
    assert_eq!(
        entry_after.queue_status, "queued",
        "our run's queue entry must remain `queued` across a second sweep"
    );
    // Spec is unchanged.
    let spec_after: TaskSpec = serde_json::from_value(entry_after.spec).unwrap();
    assert!(matches!(spec_after, TaskSpec::AutomationDecision { .. }));
}

// ── find_stuck_runs (periodic global-driver selection, Task 6 correction) ────
//
// The rung-2 invariant: the periodic loop must NEVER select a run a live
// per-request coordinator is driving. The discriminator is "has a live
// queue entry" (claimed/heart-beating), NOT task_status — which is exactly
// why `get_resumable_root_runs` is wrong for the periodic path.

async fn seed_run(db: &DatabaseConnection, source_type: &str) -> String {
    let run_id = format!("{source_type}-stuck-{}", uuid::Uuid::new_v4());
    crud::insert_run(db, &run_id, "Q", None, source_type, None, uuid::Uuid::nil())
        .await
        .unwrap();
    run_id
}

/// THE rung-2 selection invariant: a run with a `claimed` queue entry — a
/// live interactive run — must be excluded so the periodic loop cannot
/// poach it (double-drive + partial-event deletion).
#[tokio::test(flavor = "multi_thread")]
async fn find_stuck_runs_excludes_run_with_live_queue_entry() {
    let Some(db) = test_db().await else {
        return;
    };
    let run_id = seed_run(&db, "workflow").await;
    age_run(&db, &run_id, 120).await;

    // Root task claimed + heart-beating: a live scoped coordinator owns it.
    crud::enqueue_task(
        &db,
        &run_id,
        &run_id,
        None,
        &TaskSpec::AutomationDecision {
            run_id: run_id.clone(),
            pending_child_answer: None,
        },
        None,
        crud::TaskScope::Scoped,
    )
    .await
    .unwrap();
    crud::claim_task_under_root(&db, "live-coordinator", &run_id)
        .await
        .unwrap()
        .expect("scoped claim should succeed");

    let stuck = crud::find_stuck_runs(&db, 30, None).await.unwrap();
    assert!(
        !stuck.iter().any(|r| r.run_id == run_id),
        "a run with a live (claimed) queue entry must NOT be selected by \
         the periodic loop — this is the rung-2 anti-poaching invariant"
    );

    // Sanity: with no live queue entry it IS stranded.
    let other = seed_run(&db, "workflow").await;
    age_run(&db, &other, 120).await;
    let stuck = crud::find_stuck_runs(&db, 30, None).await.unwrap();
    assert!(stuck.iter().any(|r| r.run_id == other));
}

/// Generalized beyond automation: airway runs are also schedulable (Phase 2)
/// and must be picked up by the periodic loop when stranded.
#[tokio::test(flavor = "multi_thread")]
async fn find_stuck_runs_includes_airway() {
    let Some(db) = test_db().await else {
        return;
    };
    let run_id = seed_run(&db, "airway").await;
    age_run(&db, &run_id, 120).await;

    let stuck = crud::find_stuck_runs(&db, 30, None).await.unwrap();
    assert!(
        stuck.iter().any(|r| r.run_id == run_id),
        "stranded airway run must be selected"
    );
}

/// A fresh driver lease excludes a stranded run from selection so two
/// ticks / replicas don't both grab it; once stale it is selectable again.
#[tokio::test(flavor = "multi_thread")]
async fn find_stuck_runs_respects_driver_lease() {
    use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

    let Some(db) = test_db().await else {
        return;
    };
    let run_id = seed_run(&db, "workflow").await;
    age_run(&db, &run_id, 120).await;

    assert!(
        crud::try_acquire_driver(&db, &run_id, "drv-1")
            .await
            .unwrap()
    );
    let stuck = crud::find_stuck_runs(&db, 30, None).await.unwrap();
    assert!(
        !stuck.iter().any(|r| r.run_id == run_id),
        "a run with a fresh driver lease must be excluded"
    );

    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE agentic_runs SET driver_heartbeat_at = now() - make_interval(secs => $1) \
         WHERE id = $2",
        [
            (agentic_runtime::crud::DRIVER_LEASE_TTL_SECS as i32 + 60).into(),
            run_id.clone().into(),
        ],
    ))
    .await
    .unwrap();
    let stuck = crud::find_stuck_runs(&db, 30, None).await.unwrap();
    assert!(
        stuck.iter().any(|r| r.run_id == run_id),
        "a run with a stale driver lease must be selectable again"
    );
}

/// Workspace-scoped selection: when the workspace_id filter is set, only
/// runs stamped with that workspace are returned. Closes the cloud-mode
/// routing gap that motivated `agentic_runs.workspace_id` — without the
/// filter the periodic loop would drive every workspace's stranded rows
/// through whichever PlatformContext happened to win the iteration race.
///
/// Also asserts the selection helpers populate `StuckRun.workspace_id`
/// so a single shared latency worker can route per-row without re-fetching
/// the agentic_runs row just to learn which workspace it belongs to.
#[tokio::test(flavor = "multi_thread")]
async fn find_stuck_runs_scopes_to_workspace_id() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };

    let ws_a = uuid::Uuid::new_v4();
    let ws_b = uuid::Uuid::new_v4();

    let run_a = format!("ws-a-{}", uuid::Uuid::new_v4());
    let run_b = format!("ws-b-{}", uuid::Uuid::new_v4());
    crud::insert_run(&db, &run_a, "Q", None, "workflow", None, ws_a)
        .await
        .unwrap();
    crud::insert_run(&db, &run_b, "Q", None, "workflow", None, ws_b)
        .await
        .unwrap();
    age_run(&db, &run_a, 120).await;
    age_run(&db, &run_b, 120).await;

    let only_a = crud::find_stuck_runs(&db, 30, Some(ws_a)).await.unwrap();
    assert!(
        only_a.iter().any(|r| r.run_id == run_a),
        "workspace A filter must include run_a"
    );
    assert!(
        !only_a.iter().any(|r| r.run_id == run_b),
        "workspace A filter must exclude run_b (foreign workspace)"
    );
    // Round-trip workspace_id on the StuckRun struct so a shared worker
    // can dispatch per-row.
    let row_a = only_a.iter().find(|r| r.run_id == run_a).unwrap();
    assert_eq!(row_a.workspace_id, ws_a);

    let only_b = crud::find_stuck_runs(&db, 30, Some(ws_b)).await.unwrap();
    assert!(only_b.iter().any(|r| r.run_id == run_b));
    assert!(!only_b.iter().any(|r| r.run_id == run_a));

    // `None` returns both — the cloud latency worker's discovery probe.
    let all = crud::find_stuck_runs(&db, 30, None).await.unwrap();
    assert!(all.iter().any(|r| r.run_id == run_a));
    assert!(all.iter().any(|r| r.run_id == run_b));
}

/// A freshly-seeded Global run (scheduler tick / run-now) has zero
/// events and a `queued scope_owned=false` queue entry. Startup's
/// `cleanup_stale_runs` used to force-fail it as "server restarted: run
/// never started" — wrong, because it's valid pending work waiting for
/// the latency worker to pick it up. Closes the bug observed in
/// production: schedules fired via UI ended up with rows like
///   error_message='server restarted: run never started'
///   task_status='failed'
/// while their queue entries remained `queued` forever.
#[tokio::test(flavor = "multi_thread")]
async fn cleanup_stale_runs_preserves_pending_global_seed() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };

    let run_id = format!("pending-global-{}", uuid::Uuid::new_v4());
    crud::insert_run(&db, &run_id, "Q", None, "workflow", None, uuid::Uuid::nil())
        .await
        .unwrap();
    // Simulate the scheduler / run-now seed: queue entry queued,
    // scope_owned=false, no events on the run yet.
    crud::enqueue_task(
        &db,
        &run_id,
        &run_id,
        None,
        &TaskSpec::Automation {
            workflow_ref: "dummy.automation.yml".into(),
            variables: None,
            retry_from_run_id: None,
            cache_enabled: false,
            body: None,
            initial_render_context: None,
        },
        None,
        crud::TaskScope::Global,
    )
    .await
    .unwrap();

    // Pre-condition: queue row is `queued`, scope_owned=false.
    let q = crud::get_queue_entry(&db, &run_id)
        .await
        .unwrap()
        .expect("queue entry must exist");
    assert_eq!(q.queue_status, "queued");
    assert!(!q.scope_owned);

    // Run the startup cleanup. The run has zero events + parent_run_id is
    // None, so the old code would force-fail it. The fix: it sees the
    // queued queue entry and leaves the run alone.
    crud::cleanup_stale_runs(&db).await.unwrap();

    let r = crud::get_run(&db, &run_id).await.unwrap().unwrap();
    assert_eq!(
        r.task_status.as_deref(),
        Some("running"),
        "fresh Global seed must NOT be force-failed by cleanup_stale_runs; \
         got task_status={:?} error_message={:?}",
        r.task_status,
        r.error_message,
    );
    assert!(
        r.error_message.is_none(),
        "no error message should be stamped on a pending Global seed; got {:?}",
        r.error_message
    );

    // The latency worker's predicate must surface this row so the worker
    // can drive it on its next tick.
    let pending = crud::find_pending_global_runs(&db, Some(uuid::Uuid::nil()))
        .await
        .unwrap();
    assert!(
        pending.iter().any(|r| r.run_id == run_id),
        "find_pending_global_runs must include the pending Global seed"
    );
}

// ── find_pending_global_runs source-type coverage ────────────────────────────
//
// Regression: an earlier version of `find_pending_global_runs` hard-coded
// `source_type IN ('workflow', 'airway')`. When scheduled agent support
// landed, the seed function correctly enqueued a `TaskSpec::Agent` with
// `TaskScope::Global` (source_type="analytics") — but the latency worker's
// SQL silently filtered it out, so the queue row sat `queued` forever and
// the run stuck on the dashboard. The contract is: this predicate must be
// type-agnostic, because by construction the queued+!scope_owned row means
// no worker has claimed the spec yet (no "double-drive an LLM" risk that
// would justify a type filter, unlike `find_stuck_runs`).

/// Helper: seed an `agentic_runs` row + `queued scope_owned=false`
/// queue entry for a given source_type, mirroring what `start_*_run`
/// produces. Returns the run id.
async fn seed_pending_global(db: &DatabaseConnection, source_type: &str) -> String {
    let run_id = format!("pending-{source_type}-{}", uuid::Uuid::new_v4());
    crud::insert_run(db, &run_id, "Q", None, source_type, None, uuid::Uuid::nil())
        .await
        .unwrap();
    // The spec body is irrelevant to find_pending_global_runs (it reads
    // `queue_status` + `scope_owned` + `source_type` from the run row);
    // we only need *some* queued+!scope_owned row at task_id=run_id.
    let spec = match source_type {
        "workflow" => TaskSpec::Automation {
            workflow_ref: "dummy.automation.yml".into(),
            variables: None,
            retry_from_run_id: None,
            cache_enabled: false,
            body: None,
            initial_render_context: None,
        },
        "airway" => TaskSpec::Airway {
            pipeline_ref: "dummy.airway.yml".into(),
            variables: None,
            resources: Vec::new(),
            backfill_from: None,
            backfill_to: None,
        },
        "analytics" => TaskSpec::Agent {
            agent_id: "agents/dummy.agentic.yml".into(),
            question: "Q".into(),
            extra: None,
        },
        other => panic!("unsupported source_type {other:?}"),
    };
    crud::enqueue_task(
        db,
        &run_id,
        &run_id,
        None,
        &spec,
        None,
        crud::TaskScope::Global,
    )
    .await
    .unwrap();
    run_id
}

/// The contract: the latency-worker selection must be type-agnostic.
/// Automation, airway, AND analytics (agent) freshly-seeded Global runs
/// must all be picked up. Failing this test means one source type sits
/// `queued` forever and the dashboard shows it stuck.
#[tokio::test(flavor = "multi_thread")]
async fn find_pending_global_runs_picks_up_all_source_types() {
    let Some(db) = test_db().await else {
        return;
    };

    let automation = seed_pending_global(&db, "workflow").await;
    let airway = seed_pending_global(&db, "airway").await;
    let analytics = seed_pending_global(&db, "analytics").await;

    let pending = crud::find_pending_global_runs(&db, Some(uuid::Uuid::nil()))
        .await
        .unwrap();

    // Assert each individually so a failure tells you *which* source type
    // regressed rather than a vague "missing some rows".
    for (label, run_id) in [
        ("workflow", &automation),
        ("airway", &airway),
        ("analytics", &analytics),
    ] {
        assert!(
            pending.iter().any(|r| &r.run_id == run_id),
            "find_pending_global_runs must include the {label} pending Global \
             seed (run_id={run_id}); excluding this source type means \
             scheduled {label} runs sit queued forever",
        );
    }
}

/// Sanity bookend: a `queued scope_owned=true` row (interactive run's
/// not-yet-claimed task) must NOT be returned even for an analytics
/// source. The latency worker is the Global / scheduler path; the
/// scoped path has its own co-located coordinator. Without this guard
/// the worker would race the per-request coordinator and double-drive.
#[tokio::test(flavor = "multi_thread")]
async fn find_pending_global_runs_excludes_scope_owned_for_all_source_types() {
    let Some(db) = test_db().await else {
        return;
    };

    for source_type in ["workflow", "airway", "analytics"] {
        let run_id = format!("scoped-{source_type}-{}", uuid::Uuid::new_v4());
        crud::insert_run(
            &db,
            &run_id,
            "Q",
            None,
            source_type,
            None,
            uuid::Uuid::nil(),
        )
        .await
        .unwrap();
        let spec = match source_type {
            "workflow" => TaskSpec::Automation {
                workflow_ref: "dummy.automation.yml".into(),
                variables: None,
                retry_from_run_id: None,
                cache_enabled: false,
                body: None,
                initial_render_context: None,
            },
            "airway" => TaskSpec::Airway {
                pipeline_ref: "dummy.airway.yml".into(),
                variables: None,
                resources: Vec::new(),
                backfill_from: None,
                backfill_to: None,
            },
            _ => TaskSpec::Agent {
                agent_id: "agents/dummy.agentic.yml".into(),
                question: "Q".into(),
                extra: None,
            },
        };
        crud::enqueue_task(
            &db,
            &run_id,
            &run_id,
            None,
            &spec,
            None,
            crud::TaskScope::Scoped,
        )
        .await
        .unwrap();

        let pending = crud::find_pending_global_runs(&db, Some(uuid::Uuid::nil()))
            .await
            .unwrap();
        assert!(
            !pending.iter().any(|r| r.run_id == run_id),
            "find_pending_global_runs must NOT return scoped ({source_type}) \
             queue rows; that race would poach a live coordinator's task",
        );
    }
}

// ── Test: clear_run_error nulls only error_message ──────────────────────────
//
// Regression: `cleanup_stale_runs` stamps a "server restarted: run will be
// resumed automatically" note into `error_message` on every run it marks
// `needs_resume`. The frontend renders ANY non-null `error_message` as a red
// "Pipeline error" banner (`PipelineHeader.tsx`), so a healthy run that is
// actively being resumed kept showing an error. `recover_single_run` in
// `agentic-pipeline` (crates/agentic/pipeline/src/recovery.rs) now calls
// `clear_run_error` the moment it re-claims the driver lease — the single
// choke point shared by startup recovery (`recover_active_runs`), the
// periodic stranded-run sweep (`recover_stranded_runs`), and the latency
// worker (`recover_pending_global_runs`), since all three drive through
// `recover_single_run`.
//
// Unlike `reset_run_for_retry` (a full reset-in-place retry: also clears
// `answer` and the driver lease), a *resume* must NOT touch those — the
// prior answer is real state to preserve, and recovery is *acquiring* the
// lease right before this call, not releasing it. This test exercises
// `clear_run_error` directly against a seeded "just reclaimed by recovery"
// row rather than driving a full recovery pass end-to-end: the real pass
// spawns background worker/coordinator tasks that would race a synchronous
// assertion, and (per `agentic-pipeline`'s own recovery tests) a `FakePlatform`
// recovery attempt that fails downstream overwrites `error_message` again via
// `mark_recovery_failed`, which would make the assertion meaningless anyway.
// This is the altitude the task write-up calls "the clear-point function"
// test as the acceptable fallback.
#[tokio::test(flavor = "multi_thread")]
async fn clear_run_error_nulls_error_but_preserves_answer_and_driver_lease() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };

    let run_id = format!("resume-{}", uuid::Uuid::new_v4());
    crud::insert_run(
        &db,
        &run_id,
        "Q",
        None,
        "analytics",
        None,
        uuid::Uuid::nil(),
    )
    .await
    .unwrap();

    // Simulate `cleanup_stale_runs`'s placeholder plus a real prior answer
    // (e.g. a partial answer streamed before the crash) that must survive
    // the clear.
    crud::transition_run(
        &db,
        &run_id,
        "needs_resume",
        None,
        Some("partial answer from before the crash"),
        Some("server restarted: run will be resumed automatically"),
    )
    .await
    .expect("seed needs_resume + placeholder error + answer");

    // Simulate recovery's driver-lease acquisition, which happens
    // immediately before the `clear_run_error` call at the real choke point
    // (`recover_single_run` in agentic-pipeline).
    let acquired = crud::try_acquire_driver(&db, &run_id, "recovery-test-driver")
        .await
        .expect("acquire driver lease");
    assert!(acquired, "lease should be free to acquire");

    let before = crud::get_run(&db, &run_id).await.unwrap().unwrap();
    assert_eq!(
        before.error_message.as_deref(),
        Some("server restarted: run will be resumed automatically")
    );
    assert!(before.driver_id.is_some(), "lease should now be held");

    crud::clear_run_error(&db, &run_id)
        .await
        .expect("clear_run_error");

    let after = crud::get_run(&db, &run_id).await.unwrap().unwrap();
    assert_eq!(
        after.error_message, None,
        "clear_run_error must null the stale placeholder"
    );
    assert_eq!(
        after.answer.as_deref(),
        Some("partial answer from before the crash"),
        "clear_run_error must NOT touch answer — unlike reset_run_for_retry, \
         a resume preserves prior state"
    );
    assert_eq!(
        after.driver_id, before.driver_id,
        "clear_run_error must NOT touch the driver lease — recovery is \
         acquiring it here, not releasing it"
    );
    assert_eq!(
        after.driver_heartbeat_at, before.driver_heartbeat_at,
        "driver_heartbeat_at must be untouched too"
    );
    assert_eq!(
        after.task_status.as_deref(),
        Some("needs_resume"),
        "clear_run_error must not touch task_status either"
    );
}
