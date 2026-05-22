//! Tests for the stuck-run sweeper.
//!
//! The sweeper is defense-in-depth: even after `commit_decision` made the
//! decision boundary atomic, a workflow run can still be stranded if the
//! coordinator crashes between receiving a `Suspended` outcome and calling
//! `insert_child_run` + `transport.assign`. In that window the parent's
//! queue row has already been released as `completed`, so the reaper cannot
//! rescue it — its grace is keyed on `queue_status='claimed'`.
//!
//! The sweeper finds any workflow run that is still in a non-terminal
//! `task_status` but has no queue entry in `queued`/`claimed` (for itself or
//! any descendant task_id), and re-enqueues a fresh `WorkflowDecision`. The
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

async fn seed_workflow_run(db: &DatabaseConnection) -> String {
    let run_id = format!("wf-stuck-{}", uuid::Uuid::new_v4());
    crud::insert_run(db, &run_id, "Q", None, "workflow", None, uuid::Uuid::nil())
        .await
        .unwrap();
    run_id
}

/// A workflow run in `running` state with no queue row (or any descendant
/// queue row) is stranded; the sweeper should surface it.
#[tokio::test(flavor = "multi_thread")]
async fn find_stuck_runs_detects_run_with_no_queue_entry() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };

    let run_id = seed_workflow_run(&db).await;
    age_run(&db, &run_id, 60).await;

    let stuck = crud::find_stuck_workflow_runs(&db, 30).await.unwrap();
    assert!(
        stuck.iter().any(|r| r.run_id == run_id),
        "expected {run_id} in stuck runs, got: {stuck:?}"
    );
}

/// A workflow run whose child task is still `claimed` is NOT stuck — the
/// child will drive the parent forward when it finishes. The sweeper must
/// not false-positive here or it would spawn duplicate decisions.
#[tokio::test(flavor = "multi_thread")]
async fn find_stuck_runs_ignores_run_with_in_flight_child() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };

    let run_id = seed_workflow_run(&db).await;
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

    let stuck = crud::find_stuck_workflow_runs(&db, 30).await.unwrap();
    assert!(
        !stuck.iter().any(|r| r.run_id == run_id),
        "run with in-flight child must not be reported stuck"
    );
}

/// Recently-updated workflows (inside the grace window) are skipped — they
/// may be mid-commit from another worker. Acting on them would race.
#[tokio::test(flavor = "multi_thread")]
async fn find_stuck_runs_respects_grace_window() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };

    let run_id = seed_workflow_run(&db).await;
    // No age_run — run was just created; updated_at is near now().

    let stuck = crud::find_stuck_workflow_runs(&db, 30).await.unwrap();
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

    let run_id = seed_workflow_run(&db).await;
    age_run(&db, &run_id, 60).await;
    crud::update_run_done(&db, &run_id, "done", None)
        .await
        .unwrap();
    age_run(&db, &run_id, 60).await; // ensure still outside grace after update

    let stuck = crud::find_stuck_workflow_runs(&db, 30).await.unwrap();
    assert!(!stuck.iter().any(|r| r.run_id == run_id));
}

/// End-to-end: `run_stuck_run_sweeper` on a `DurableTransport` re-enqueues a
/// `WorkflowDecision` for each stuck run. The queue row is upsert-safe, so a
/// second sweep must not re-rescue the same run.
///
/// The test DB is shared across tests via a reused testcontainer, so the
/// sweep may also rescue runs seeded by other tests. The assertion is
/// scoped to this test's own run to stay isolated.
#[tokio::test(flavor = "multi_thread")]
async fn sweeper_re_enqueues_workflow_decision_idempotently() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };
    let run_id = seed_workflow_run(&db).await;
    age_run(&db, &run_id, 60).await;

    let transport = DurableTransport::with_config(db.clone(), Duration::from_millis(100));

    // No queue row before sweeping.
    assert!(crud::get_queue_entry(&db, &run_id).await.unwrap().is_none());

    let rescued = transport.run_stuck_run_sweeper(30).await;
    assert!(rescued >= 1, "expected to rescue at least this run");

    // Queue row now present with a WorkflowDecision spec for this run.
    let entry = crud::get_queue_entry(&db, &run_id).await.unwrap().unwrap();
    assert_eq!(entry.queue_status, "queued");
    let spec: TaskSpec = serde_json::from_value(entry.spec).unwrap();
    assert!(
        matches!(spec, TaskSpec::WorkflowDecision { .. }),
        "expected WorkflowDecision spec"
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
    assert!(matches!(spec_after, TaskSpec::WorkflowDecision { .. }));
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
        &TaskSpec::WorkflowDecision {
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

/// Generalized beyond workflow: airway runs are also schedulable (Phase 2)
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
        &TaskSpec::Workflow {
            workflow_ref: "dummy.workflow.yml".into(),
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
