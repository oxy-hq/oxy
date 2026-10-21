//! Boot cleanup against runs another process is driving.
//!
//! Two "processes" share one database: process A drives a run (it holds the
//! driver lease, or a queue claim in the run's tree), process B boots and runs
//! `cleanup_stale_runs` plus the startup selection `get_resumable_root_runs`.
//! B must leave A's run exactly as it is; a run whose driver died (lease past
//! the TTL, nothing in the queue) is still B's to reconcile and resume; and a
//! status a peer made terminal while B was mid-cleanup is never overwritten.
//!
//! Run:
//!   cargo nextest run -p agentic-runtime --test integration -E 'test(boot_cleanup_liveness_test)'

use agentic_core::delegation::TaskSpec;
use agentic_runtime::crud;
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, TransactionTrait};

use crate::stuck_run_sweeper_test::{seed_run, test_db};

/// Process A's driver id, as `lifecycle::driver_lease` mints them.
const DRIVER_A: &str = "direct-process-a";

async fn add_event(db: &DatabaseConnection, run_id: &str) {
    crud::insert_event(db, run_id, 0, "step_start", &serde_json::json!({}), 0)
        .await
        .unwrap();
}

/// `(task_status, error_message, updated_at)` — the three columns a stamp moves.
async fn snapshot(
    db: &DatabaseConnection,
    run_id: &str,
) -> (Option<String>, Option<String>, String) {
    let run = crud::get_run(db, run_id).await.unwrap().expect("run row");
    (
        run.task_status,
        run.error_message,
        run.updated_at.to_rfc3339(),
    )
}

async fn resumable(db: &DatabaseConnection, run_id: &str) -> bool {
    crud::get_resumable_root_runs(db, Some(uuid::Uuid::nil()))
        .await
        .unwrap()
        .iter()
        .any(|r| r.id == run_id)
}

/// Push the lease's last heartbeat `secs` into the past, on the DB clock.
async fn age_lease(db: &DatabaseConnection, run_id: &str, secs: i64) {
    db.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE agentic_runs \
         SET driver_heartbeat_at = now() - make_interval(secs => $1) WHERE id = $2",
        [(secs as i32).into(), run_id.into()],
    ))
    .await
    .unwrap();
}

/// A run process A is direct-driving: it has events and A holds the lease.
async fn seed_driven_by_a(db: &DatabaseConnection, kind: &str) -> String {
    let run_id = seed_run(db, kind).await;
    add_event(db, &run_id).await;
    assert!(
        crud::try_acquire_driver(db, &run_id, DRIVER_A)
            .await
            .unwrap()
    );
    run_id
}

/// Process B booting leaves a run A is driving untouched — no `needs_resume`
/// stamp, no note, no `updated_at` bump — and its startup pass does not
/// select it.
#[tokio::test(flavor = "multi_thread")]
async fn a_booting_peer_leaves_a_run_another_process_is_driving() {
    let Some(db) = test_db().await else {
        return;
    };
    let run_id = seed_driven_by_a(&db, "analytics").await;
    let before = snapshot(&db, &run_id).await;

    crud::cleanup_stale_runs(&db).await.unwrap();

    assert_eq!(
        snapshot(&db, &run_id).await,
        before,
        "process B's boot must not stamp a run process A holds the lease on"
    );
    assert!(
        !resumable(&db, &run_id).await,
        "process B's startup pass must not select it either"
    );
}

/// The same for a zero-event root: a fresh lease is A preparing it, not a run
/// that never started.
#[tokio::test(flavor = "multi_thread")]
async fn a_leased_root_with_no_events_is_not_failed() {
    let Some(db) = test_db().await else {
        return;
    };
    let run_id = seed_run(&db, "analytics").await;
    assert!(
        crud::try_acquire_driver(&db, &run_id, DRIVER_A)
            .await
            .unwrap()
    );

    crud::cleanup_stale_runs(&db).await.unwrap();

    assert_eq!(
        snapshot(&db, &run_id).await.0.as_deref(),
        Some("running"),
        "a root a live driver holds is not 'never started'"
    );
}

/// A run held through its task tree rather than a lease — a worker on another
/// pod claimed a child task — is left alone too.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_whose_child_task_a_worker_holds_is_not_stamped() {
    let Some(db) = test_db().await else {
        return;
    };
    let run_id = seed_run(&db, "workflow").await;
    add_event(&db, &run_id).await;
    let child = format!("{run_id}.1");
    crud::enqueue_task(
        &db,
        &child,
        &run_id,
        Some(&run_id),
        &TaskSpec::Custom {
            kind: "boot_cleanup_child".into(),
            payload: serde_json::json!({}),
        },
        None,
        crud::TaskScope::Global,
    )
    .await
    .unwrap();
    crud::claim_task_under_root(&db, "worker-on-another-pod", &run_id)
        .await
        .unwrap()
        .expect("the child is claimable");
    let before = snapshot(&db, &run_id).await;

    crud::cleanup_stale_runs(&db).await.unwrap();

    assert_eq!(snapshot(&db, &run_id).await, before);
}

/// A run whose driver died — its lease went past the TTL and nothing holds it
/// in the queue — is still recovered: stamped and selected.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_whose_driver_died_is_recovered() {
    let Some(db) = test_db().await else {
        return;
    };
    let run_id = seed_driven_by_a(&db, "analytics").await;
    age_lease(&db, &run_id, crud::DRIVER_LEASE_TTL_SECS + 30).await;

    crud::cleanup_stale_runs(&db).await.unwrap();

    assert_eq!(
        snapshot(&db, &run_id).await.0.as_deref(),
        Some("needs_resume")
    );
    assert!(resumable(&db, &run_id).await, "the startup pass resumes it");

    // And a dead driver's zero-event root is still failed as never started.
    let never = seed_run(&db, "analytics").await;
    assert!(
        crud::try_acquire_driver(&db, &never, DRIVER_A)
            .await
            .unwrap()
    );
    age_lease(&db, &never, crud::DRIVER_LEASE_TTL_SECS + 30).await;
    crud::cleanup_stale_runs(&db).await.unwrap();
    assert_eq!(snapshot(&db, &never).await.0.as_deref(), Some("failed"));
}

/// A peer finishes the run while B's cleanup is between reading it and
/// writing it. The peer's `done` is held uncommitted under a row lock until
/// the cleanup has read the row as `running`, then committed: the cleanup's
/// write must not land on top of it.
#[tokio::test(flavor = "multi_thread")]
async fn a_peers_done_is_never_overwritten() {
    let Some(db) = test_db().await else {
        return;
    };
    // Unleased, with events: exactly what the cleanup stamps.
    let run_id = seed_run(&db, "analytics").await;
    add_event(&db, &run_id).await;

    let peer = db.begin().await.unwrap();
    peer.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE agentic_runs SET task_status = 'done', answer = 'peer finished' WHERE id = $1",
        [run_id.clone().into()],
    ))
    .await
    .unwrap();

    let cleanup_db = db.clone();
    let cleanup = tokio::spawn(async move { crud::cleanup_stale_runs(&cleanup_db).await });
    // Let the cleanup read the row and block on the peer's lock.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    peer.commit().await.unwrap();
    cleanup.await.unwrap().unwrap();

    let run = crud::get_run(&db, &run_id).await.unwrap().unwrap();
    assert_eq!(
        run.task_status.as_deref(),
        Some("done"),
        "the peer's status stands"
    );
    assert_eq!(
        run.error_message, None,
        "and no restart note is added to it"
    );
}

/// A child whose parent ended is failed — unless it already reached a terminal
/// status of its own.
#[tokio::test(flavor = "multi_thread")]
async fn an_orphan_child_is_failed_but_a_finished_one_is_not() {
    let Some(db) = test_db().await else {
        return;
    };
    let parent = seed_run(&db, "analytics").await;
    crud::update_run_done(&db, &parent, "done", None)
        .await
        .unwrap();
    let mut children = Vec::new();
    for status in ["running", "done"] {
        let child = format!("{parent}.{}", children.len() + 1);
        crud::insert_run_with_parent(
            &db,
            &child,
            &parent,
            "Q",
            "analytics",
            None,
            0,
            uuid::Uuid::nil(),
        )
        .await
        .unwrap();
        crud::update_task_status(&db, &child, status, None)
            .await
            .unwrap();
        children.push(child);
    }

    crud::cleanup_stale_runs(&db).await.unwrap();

    assert_eq!(
        snapshot(&db, &children[0]).await.0.as_deref(),
        Some("failed")
    );
    assert_eq!(snapshot(&db, &children[1]).await.0.as_deref(), Some("done"));
}
