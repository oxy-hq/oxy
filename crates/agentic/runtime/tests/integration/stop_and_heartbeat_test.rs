//! Stop must always fire its in-memory tokens, and a heartbeat ticker must
//! survive a failed write but stop the task when the claim is lost.
//!
//! Run:
//!   cargo nextest run -p agentic-runtime --test integration -E 'test(stop_and_heartbeat_test)'

use std::time::Duration;

use agentic_core::delegation::TaskOutcome;
use agentic_core::transport::{CoordinatorTransport, WorkerMessage, WorkerTransport};
use agentic_runtime::crud::queue::TaskScope;
use agentic_runtime::orchestrator::transport::{DurableTransport, process_worker_id};
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};

use super::eviction_safety_test::{seed_task, test_db};

/// Tick for tickers that must reach the database. Each beat is time-boxed at
/// 60% of the interval, so a 20ms tick gives a real round-trip 12ms, which a
/// loaded machine misses on every beat and the ticker never acts.
const DB_TICK: Duration = Duration::from_millis(1000);

/// A connection whose pool is closed: every statement fails.
async fn closed_db() -> DatabaseConnection {
    let db = test_db().await.expect("test db");
    db.clone().close().await.expect("close pool");
    db
}

#[tokio::test]
async fn cancel_fires_token_even_when_db_write_fails() {
    let transport = DurableTransport::new(closed_db().await);
    let token = transport.cancellation_token("t-1");

    let res = transport.cancel("t-1").await;

    assert!(res.is_err(), "the DB write must still surface its error");
    assert!(token.is_cancelled(), "Stop must reach the running task");
}

#[tokio::test]
async fn cancel_subtree_fires_root_and_descendants_even_when_db_write_fails() {
    let transport = DurableTransport::new(closed_db().await);
    let root = transport.cancellation_token("r-1");
    let child = transport.cancellation_token("r-1.1");
    let grandchild = transport.cancellation_token("r-1.1.2");
    let other = transport.cancellation_token("r-10.1");

    let res = transport.cancel_subtree("r-1").await;

    assert!(res.is_err(), "the DB write must still surface its error");
    assert!(root.is_cancelled());
    assert!(child.is_cancelled());
    assert!(grandchild.is_cancelled());
    assert!(!other.is_cancelled(), "a sibling run must not be touched");
}

#[tokio::test]
async fn heartbeat_ticker_survives_failed_writes() {
    let transport = DurableTransport::new(closed_db().await);
    let task_token = transport.cancellation_token("hb-1");

    let ticker = transport.spawn_heartbeat("hb-1", Duration::from_millis(20));
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert!(
        !ticker.is_cancelled(),
        "one failed beat must not end the ticker"
    );
    assert!(
        !task_token.is_cancelled(),
        "a failed write is not a lost claim"
    );
    ticker.cancel();
}

#[tokio::test]
async fn heartbeat_lost_claim_stops_ticker_and_cancels_task() {
    let db = test_db().await.expect("test db");
    let (_run, task_id) = seed_task(&db, "test", TaskScope::Global).await;
    // Claimed by a different worker than the transport below.
    db.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE agentic_task_queue SET queue_status = 'claimed', worker_id = 'someone-else' \
         WHERE task_id = $1",
        [task_id.clone().into()],
    ))
    .await
    .unwrap();

    let transport = DurableTransport::new(db);
    let task_token = transport.cancellation_token(&task_id);
    let ticker = transport.spawn_heartbeat(&task_id, DB_TICK);

    tokio::time::timeout(Duration::from_secs(5), task_token.cancelled())
        .await
        .expect("a lost claim must cancel the executing task");
    assert!(ticker.is_cancelled(), "and the ticker must stop");
}

/// Interactive roots registered via `Coordinator::register_root` have no queue
/// row, yet their virtual worker still spawns a heartbeat. "No row" is not a
/// lost claim: the ticker must stop quietly and never cancel the task.
#[tokio::test]
async fn heartbeat_with_no_queue_row_does_not_cancel_the_task() {
    let db = test_db().await.expect("test db");
    let task_id = format!("never-enqueued-{}", uuid::Uuid::new_v4());

    let transport = DurableTransport::new(db);
    let task_token = transport.cancellation_token(&task_id);
    let ticker = transport.spawn_heartbeat(&task_id, DB_TICK);

    tokio::time::timeout(Duration::from_secs(5), ticker.cancelled())
        .await
        .expect("the ticker should stop on a row that does not exist");
    assert!(
        !task_token.is_cancelled(),
        "a task with no queue row must not be cancelled by its heartbeat"
    );
}

/// The reaper requeues a row whose claim was lost, and this same transport
/// re-claims it through `recv_assignment`, which installs no token of its own.
/// The lost claim's cancel must stop *that* execution only: the next claim of
/// the same task_id has to start with a live token, or it runs `Cancelled`.
#[tokio::test]
async fn heartbeat_lost_claim_leaves_a_live_token_for_the_next_claim() {
    let db = test_db().await.expect("test db");
    let (_run, task_id) = seed_task(&db, "test", TaskScope::Global).await;
    db.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE agentic_task_queue SET queue_status = 'claimed', worker_id = 'someone-else' \
         WHERE task_id = $1",
        [task_id.clone().into()],
    ))
    .await
    .unwrap();

    let transport = DurableTransport::new(db);
    let old_claim = transport.cancellation_token(&task_id);
    let _ticker = transport.spawn_heartbeat(&task_id, DB_TICK);

    tokio::time::timeout(Duration::from_secs(30), old_claim.cancelled())
        .await
        .expect("a lost claim must cancel the executing task");
    assert!(
        !transport.cancellation_token(&task_id).is_cancelled(),
        "the next claim of this task must not inherit the lost claim's cancel"
    );
}

/// Seed a row, set its queue state, and let this transport's ticker read
/// `Lost` and cancel the task. Returns the transport, its database and the task id.
async fn lose_claim(
    row_update: &str,
) -> (std::sync::Arc<DurableTransport>, DatabaseConnection, String) {
    let db = test_db().await.expect("test db");
    let (_run, task_id) = seed_task(&db, "test", TaskScope::Global).await;
    db.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        row_update,
        [task_id.clone().into()],
    ))
    .await
    .unwrap();

    let transport = DurableTransport::new(db.clone());
    let task_token = transport.cancellation_token(&task_id);
    let _ticker = transport.spawn_heartbeat(&task_id, DB_TICK);
    tokio::time::timeout(Duration::from_secs(30), task_token.cancelled())
        .await
        .expect("a lost claim must cancel the executing task");
    (transport, db, task_id)
}

async fn forwarded(transport: &DurableTransport) -> Option<WorkerMessage> {
    tokio::time::timeout(Duration::from_secs(2), transport.recv())
        .await
        .ok()
        .flatten()
}

/// The reaper requeued the row (or a peer holds it): the task was cancelled
/// only because its claim was lost, so its `Cancelled` must not reach the
/// coordinator — that would mark the run cancelled while a peer re-runs it.
#[tokio::test]
async fn lost_claim_cancel_is_not_forwarded_as_an_outcome() {
    let (transport, _db, task_id) = lose_claim(
        "UPDATE agentic_task_queue SET queue_status = 'claimed', worker_id = 'someone-else' \
         WHERE task_id = $1",
    )
    .await;

    transport
        .send(WorkerMessage::Outcome {
            task_id,
            outcome: TaskOutcome::Cancelled,
        })
        .await
        .unwrap();

    assert!(
        forwarded(&transport).await.is_none(),
        "a lost-claim cancel must not reach the coordinator"
    );
}

/// A Stop reaches this task through the queue when it was issued from another
/// process, or after the reaper took the row: the row goes `cancelled` and the
/// beat reads `Lost`. Nobody else will report that run, so its `Cancelled`
/// must still reach the coordinator, even though the write reads `NotOwned`.
#[tokio::test]
async fn peer_stop_seen_as_lost_claim_is_still_forwarded() {
    let (transport, _db, task_id) =
        lose_claim("UPDATE agentic_task_queue SET queue_status = 'cancelled' WHERE task_id = $1")
            .await;

    transport
        .send(WorkerMessage::Outcome {
            task_id: task_id.clone(),
            outcome: TaskOutcome::Cancelled,
        })
        .await
        .unwrap();

    match forwarded(&transport).await {
        Some(WorkerMessage::Outcome {
            task_id: id,
            outcome: TaskOutcome::Cancelled,
        }) => assert_eq!(id, task_id),
        other => panic!("a peer's Stop must reach the coordinator, got {other:?}"),
    }
}

/// Put the row back under this process's own claim, as `recv_assignment` does
/// when the reaper requeued it: `worker_id` is process-stable, so the new
/// claim looks exactly like the lost one to every owned write.
async fn reclaim_as_this_process(db: &DatabaseConnection, task_id: &str) {
    db.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE agentic_task_queue SET queue_status = 'claimed', worker_id = $2 \
         WHERE task_id = $1",
        [task_id.into(), process_worker_id().into()],
    ))
    .await
    .unwrap();
}

async fn queue_status(db: &DatabaseConnection, task_id: &str) -> String {
    agentic_runtime::crud::get_queue_entry(db, task_id)
        .await
        .unwrap()
        .expect("queue row")
        .queue_status
}

const CLAIMED_BY_PEER: &str = "UPDATE agentic_task_queue \
     SET queue_status = 'claimed', worker_id = 'someone-else' WHERE task_id = $1";

/// The lost execution's `Cancelled` lands after this process re-claimed the
/// row as a new execution. It must not stamp the new execution's row, retire
/// its ticker, or reach the coordinator.
#[tokio::test]
async fn lost_claim_cancel_leaves_this_process_reclaim_alone() {
    let (transport, db, task_id) = lose_claim(CLAIMED_BY_PEER).await;
    reclaim_as_this_process(&db, &task_id).await;
    let new_ticker = transport.spawn_heartbeat(&task_id, DB_TICK);

    transport
        .send(WorkerMessage::Outcome {
            task_id: task_id.clone(),
            outcome: TaskOutcome::Cancelled,
        })
        .await
        .unwrap();

    assert!(
        forwarded(&transport).await.is_none(),
        "the lost execution's cancel must not reach the coordinator"
    );
    assert_eq!(queue_status(&db, &task_id).await, "claimed");
    assert!(
        !new_ticker.is_cancelled(),
        "the re-claim's ticker must keep beating"
    );
    new_ticker.cancel();
}

/// The re-claim finishes first. Its `Done` is not the lost execution's
/// cancel, so it must stamp its own row and be reported as usual.
#[tokio::test]
async fn reclaim_done_still_stamps_after_a_lost_claim() {
    let (transport, db, task_id) = lose_claim(CLAIMED_BY_PEER).await;
    reclaim_as_this_process(&db, &task_id).await;
    let _new_ticker = transport.spawn_heartbeat(&task_id, DB_TICK);

    transport
        .send(WorkerMessage::Outcome {
            task_id: task_id.clone(),
            outcome: TaskOutcome::Done {
                answer: "ok".into(),
                metadata: None,
            },
        })
        .await
        .unwrap();

    assert_eq!(queue_status(&db, &task_id).await, "completed");
    assert!(matches!(
        forwarded(&transport).await,
        Some(WorkerMessage::Outcome {
            outcome: TaskOutcome::Done { .. },
            ..
        })
    ));
}
