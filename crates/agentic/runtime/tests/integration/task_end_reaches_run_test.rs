//! Every way a task ends must reach its run: a dead-letter (deferral ceiling
//! or reaper) and a panic while the task is starting each produce one `Failed`
//! outcome, and the run reaches a terminal status. The heartbeat covers the
//! start too, so a slow `execute` is not mistaken for a dead worker.
//!
//! Hardening plan items 6 and 9 (`internal-docs/2026-10-13-orchestrator-hardening-plan.md`).
//!
//! Run:
//!   cargo nextest run -p agentic-runtime --test integration -E 'test(task_end_reaches_run_test)'

use std::sync::{Arc, Mutex};
use std::time::Duration;

use agentic_core::delegation::{ClaimEpoch, TaskAssignment, TaskOutcome, TaskSpec};
use agentic_core::transport::{
    CoordinatorTransport, TransportError, WorkerMessage, WorkerTransport,
};
use agentic_runtime::coordinator::Coordinator;
use agentic_runtime::crud;
use agentic_runtime::crud::queue::TaskScope;
use agentic_runtime::orchestrator::transport::DurableTransport;
use agentic_runtime::state::RuntimeState;
use agentic_runtime::worker::{ExecutingTask, TaskExecutor, Worker};
use async_trait::async_trait;
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};
use tokio::sync::{Notify, mpsc};
use tokio_util::sync::CancellationToken;

use super::eviction_safety_test::{seed_task, test_db};

/// Generous: the machine running these is heavily loaded.
const SETTLE: Duration = Duration::from_secs(30);

async fn exec(db: &DatabaseConnection, sql: &str, id: &str) {
    db.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [id.into()],
    ))
    .await
    .unwrap();
}

/// Make the claim on `task_id` look abandoned with its retry budget spent, so
/// the next reaper pass dead-letters it.
async fn exhaust_and_stall(db: &DatabaseConnection, task_id: &str) {
    exec(
        db,
        "UPDATE agentic_task_queue SET claim_count = max_claims, \
         last_heartbeat = now() - interval '10 minutes' WHERE task_id = $1",
        task_id,
    )
    .await;
}

async fn run_row(db: &DatabaseConnection, run_id: &str) -> agentic_runtime::entity::run::Model {
    crud::get_run(db, run_id).await.unwrap().expect("run row")
}

fn is_terminal(status: Option<&str>) -> bool {
    matches!(status, Some("done" | "failed" | "cancelled" | "timed_out"))
}

fn agent_spec() -> TaskSpec {
    TaskSpec::Agent {
        agent_id: "test-agent".into(),
        question: "q".into(),
        extra: None,
    }
}

/// Insert a run and drive it with a real coordinator and worker over a scoped
/// durable transport. Returns once the coordinator finishes, or `false` if it
/// is still waiting after [`SETTLE`].
async fn drive_to_end(
    db: &DatabaseConnection,
    run_id: &str,
    worker_transport: Arc<dyn WorkerTransport>,
    transport: Arc<DurableTransport>,
    executor: Arc<dyn TaskExecutor>,
) -> bool {
    crud::insert_run(db, run_id, "q", None, "analytics", None, uuid::Uuid::nil())
        .await
        .unwrap();
    let mut coordinator = Coordinator::new(
        db.clone(),
        Arc::new(RuntimeState::new()),
        transport.clone() as Arc<dyn CoordinatorTransport>,
    );
    coordinator
        .submit_root(run_id.to_string(), agent_spec())
        .await
        .unwrap();
    let worker = Worker::new(worker_transport, executor);
    let worker_handle = tokio::spawn(async move { worker.run().await });
    let finished = tokio::time::timeout(SETTLE, async move { coordinator.run().await })
        .await
        .is_ok();
    transport.retire_worker_loop();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_handle).await;
    finished
}

// ── Item 6: dead-letter ──────────────────────────────────────────────────────

/// Reports one deferral whose ceiling is already reached, so the queue
/// dead-letters the task on its first defer.
struct DeferPastCeiling;

#[async_trait]
impl TaskExecutor for DeferPastCeiling {
    async fn execute(&self, _a: TaskAssignment) -> Result<ExecutingTask, String> {
        let (_event_tx, events) = mpsc::channel(1);
        let (outcome_tx, outcomes) = mpsc::channel(1);
        outcome_tx
            .send(TaskOutcome::Deferred {
                delay_secs: 0,
                max_wait_secs: 0,
                reason: "pipeline lease held by another run".into(),
            })
            .await
            .unwrap();
        Ok(ExecutingTask {
            events,
            outcomes,
            cancel: CancellationToken::new(),
            answers: None,
        })
    }
}

/// The defer path dead-letters the task. The coordinator driving the run must
/// hear it as a `Failed` outcome — it used to wait for ever, keeping its
/// scoped worker polling the queue until the process exited.
#[tokio::test]
async fn a_deferral_dead_letter_reaches_the_coordinator_and_fails_the_run() {
    let db = test_db().await.expect("test db");
    let run_id = format!("dl-defer-{}", uuid::Uuid::new_v4());
    let transport = DurableTransport::new_scoped(db.clone(), run_id.clone());

    let finished = drive_to_end(
        &db,
        &run_id,
        transport.clone() as Arc<dyn WorkerTransport>,
        transport,
        Arc::new(DeferPastCeiling),
    )
    .await;

    assert!(
        finished,
        "the coordinator never heard that its task was dead-lettered; it is still \
         waiting, and its worker is still polling the queue"
    );
    let run = run_row(&db, &run_id).await;
    assert_eq!(run.task_status.as_deref(), Some("failed"));
    let msg = run.error_message.unwrap_or_default();
    assert!(
        msg.contains("dead-lettered"),
        "error names the dead-letter: {msg}"
    );
    assert!(
        msg.contains("pipeline lease held by another run"),
        "and carries the domain's reason: {msg}"
    );
}

/// The reaper dead-letters a claim whose worker is gone, on a run nobody is
/// driving. Nothing else will ever select that run, so the reaper must end it.
#[tokio::test]
async fn a_reaper_dead_letter_fails_a_run_nobody_drives() {
    let db = test_db().await.expect("test db");
    let (run_id, task_id) = seed_task(&db, "analytics", TaskScope::Global).await;
    crud::claim_task_under_root(&db, "gone-worker", &task_id)
        .await
        .unwrap()
        .expect("claim");
    exhaust_and_stall(&db, &task_id).await;

    crud::reap_stale_tasks(&db).await.unwrap();

    let row = crud::get_queue_entry(&db, &task_id).await.unwrap().unwrap();
    assert_eq!(
        row.queue_status, "dead",
        "precondition: the reaper dead-lettered it"
    );
    let run = run_row(&db, &run_id).await;
    assert_eq!(
        run.task_status.as_deref(),
        Some("failed"),
        "a dead-lettered task's run must not keep reading as in flight"
    );
    let msg = run.error_message.unwrap_or_default();
    assert!(
        msg.contains("dead-lettered"),
        "error names the dead-letter: {msg}"
    );
}

/// The same dead-letter on a run a live process is driving: the reaper must
/// leave the run to that driver, and the driver — whose task is cancelled by
/// its heartbeat finding the claim gone — must report a `Failed` naming the
/// dead-letter, not a `Cancelled` that reads like a user's Stop.
#[tokio::test]
async fn a_reaper_dead_letter_under_a_live_driver_is_reported_by_that_driver() {
    let db = test_db().await.expect("test db");
    let (run_id, task_id) = seed_task(&db, "analytics", TaskScope::Global).await;
    exec(
        &db,
        "UPDATE agentic_runs SET driver_id = 'live-driver', driver_heartbeat_at = now() \
         WHERE id = $1",
        &run_id,
    )
    .await;
    let transport = DurableTransport::new_scoped(db.clone(), task_id.clone());
    let assignment = tokio::time::timeout(SETTLE, transport.recv_assignment())
        .await
        .expect("claim")
        .expect("assignment");
    exhaust_and_stall(&db, &task_id).await;

    crud::reap_stale_tasks(&db).await.unwrap();

    let run = run_row(&db, &run_id).await;
    assert!(
        !is_terminal(run.task_status.as_deref()),
        "a run a live driver holds is that driver's to end, got {:?}",
        run.task_status
    );

    // The driver's task stops (its heartbeat reads the claim as lost) and
    // reports what a cancelled task reports.
    transport
        .send(WorkerMessage::Outcome {
            task_id: task_id.clone(),
            outcome: TaskOutcome::Cancelled,
            claim_epoch: assignment.claim_epoch,
        })
        .await
        .unwrap();
    match tokio::time::timeout(Duration::from_secs(5), transport.recv()).await {
        Ok(Some(WorkerMessage::Outcome {
            outcome: TaskOutcome::Failed(msg),
            ..
        })) => assert!(
            msg.contains("dead-lettered"),
            "names the dead-letter: {msg}"
        ),
        other => panic!("the driver must hear a Failed naming the dead-letter, got {other:?}"),
    }
}

// ── Item 9: the start of a task ─────────────────────────────────────────────

/// Forwards to a durable transport, recording every heartbeat ticker the
/// worker asks for and optionally shortening its interval.
struct SpyTransport {
    inner: Arc<DurableTransport>,
    interval: Option<Duration>,
    tickers: Mutex<Vec<CancellationToken>>,
}

impl SpyTransport {
    fn new(inner: Arc<DurableTransport>, interval: Option<Duration>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            interval,
            tickers: Mutex::new(Vec::new()),
        })
    }

    fn tickers(&self) -> Vec<CancellationToken> {
        self.tickers.lock().unwrap().clone()
    }
}

#[async_trait]
impl WorkerTransport for SpyTransport {
    async fn recv_assignment(&self) -> Option<TaskAssignment> {
        self.inner.recv_assignment().await
    }
    async fn send(&self, msg: WorkerMessage) -> Result<(), TransportError> {
        self.inner.send(msg).await
    }
    fn cancellation_token(&self, task_id: &str, claim: ClaimEpoch) -> CancellationToken {
        self.inner.cancellation_token(task_id, claim)
    }
    fn spawn_heartbeat(
        &self,
        task_id: &str,
        claim: ClaimEpoch,
        interval: Duration,
    ) -> CancellationToken {
        let token = self
            .inner
            .spawn_heartbeat(task_id, claim, self.interval.unwrap_or(interval));
        self.tickers.lock().unwrap().push(token.clone());
        token
    }
}

struct PanicsWhileStarting;

#[async_trait]
impl TaskExecutor for PanicsWhileStarting {
    async fn execute(&self, _a: TaskAssignment) -> Result<ExecutingTask, String> {
        panic!("connector handshake blew up");
    }
}

/// A panic inside `execute` used to unwind the worker's task with no outcome:
/// the coordinator waited for ever and the run read `running`.
#[tokio::test]
async fn a_panic_while_starting_fails_the_run_with_its_message() {
    let db = test_db().await.expect("test db");
    let run_id = format!("panic-{}", uuid::Uuid::new_v4());
    let transport = DurableTransport::new_scoped(db.clone(), run_id.clone());
    let spy = SpyTransport::new(transport.clone(), None);

    let finished = drive_to_end(
        &db,
        &run_id,
        spy.clone() as Arc<dyn WorkerTransport>,
        transport,
        Arc::new(PanicsWhileStarting),
    )
    .await;

    assert!(
        finished,
        "a panic in execute must still reach the coordinator as an outcome"
    );
    let run = run_row(&db, &run_id).await;
    assert_eq!(run.task_status.as_deref(), Some("failed"));
    let msg = run.error_message.unwrap_or_default();
    assert!(
        msg.contains("connector handshake blew up"),
        "the run carries the panic message: {msg}"
    );
    let row = crud::get_queue_entry(&db, &run_id).await.unwrap().unwrap();
    assert_eq!(row.queue_status, "failed");
    let tickers = spy.tickers();
    assert_eq!(tickers.len(), 1, "one ticker for the one claim");
    tokio::time::timeout(Duration::from_secs(5), tickers[0].cancelled())
        .await
        .expect("the heartbeat must retire once the task has ended");
}

/// Parks inside `execute` until released, standing in for a slow setup (a
/// compile wait, a connector handshake).
struct SlowStart {
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait]
impl TaskExecutor for SlowStart {
    async fn execute(&self, _a: TaskAssignment) -> Result<ExecutingTask, String> {
        self.entered.notify_one();
        self.release.notified().await;
        let (_event_tx, events) = mpsc::channel(1);
        let (outcome_tx, outcomes) = mpsc::channel(1);
        outcome_tx
            .send(TaskOutcome::Done {
                answer: "ok".into(),
                metadata: None,
            })
            .await
            .unwrap();
        Ok(ExecutingTask {
            events,
            outcomes,
            cancel: CancellationToken::new(),
            answers: None,
        })
    }
}

/// The heartbeat used to start only after `execute` returned, so a setup
/// longer than the visibility timeout read as a dead worker and the reaper
/// handed a live task to someone else.
#[tokio::test]
async fn the_claim_is_heartbeating_while_execute_is_still_starting() {
    let db = test_db().await.expect("test db");
    let (_run_id, task_id) = seed_task(&db, "analytics", TaskScope::Global).await;
    let transport = DurableTransport::new_scoped(db.clone(), task_id.clone());
    let spy = SpyTransport::new(transport.clone(), Some(Duration::from_secs(1)));
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let worker = Worker::new(
        spy.clone() as Arc<dyn WorkerTransport>,
        Arc::new(SlowStart {
            entered: entered.clone(),
            release: release.clone(),
        }),
    );
    let worker_handle = tokio::spawn(async move { worker.run().await });
    tokio::time::timeout(SETTLE, entered.notified())
        .await
        .expect("the worker claimed the task and entered execute");

    // Long silence, as if the setup had been running for minutes; a live
    // ticker overwrites it within one interval.
    exec(
        &db,
        "UPDATE agentic_task_queue SET last_heartbeat = now() - interval '10 minutes' \
         WHERE task_id = $1",
        &task_id,
    )
    .await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    crud::reap_stale_tasks(&db).await.unwrap();

    let row = crud::get_queue_entry(&db, &task_id).await.unwrap().unwrap();
    release.notify_one();
    assert_eq!(
        row.queue_status, "claimed",
        "a task still inside execute was reaped as if its worker were dead"
    );
    assert_eq!(
        row.worker_id.as_deref(),
        Some(agentic_runtime::transport::process_worker_id())
    );

    let done = tokio::time::timeout(SETTLE, async {
        loop {
            let s = crud::get_queue_entry(&db, &task_id).await.unwrap().unwrap();
            if s.queue_status == "completed" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await;
    assert!(done.is_ok(), "the task finishes once its setup returns");
    transport.retire_worker_loop();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_handle).await;
}
