//! Durable transport backed by the `agentic_task_queue` database table.
//!
//! Assignments are INSERT-ed by the coordinator and claimed by workers via
//! `FOR UPDATE SKIP LOCKED`. Worker→coordinator messages still flow through
//! an in-memory channel (they are already persisted by the coordinator on
//! receipt). Only the assignment direction needs durability — the single gap
//! that existed with [`super::LocalTransport`].

use std::sync::Arc;
use std::time::Duration;

use agentic_core::delegation::{TaskAssignment, TaskOutcome, TaskSpec};
use agentic_core::transport::{
    CoordinatorTransport, TransportError, WorkerMessage, WorkerTransport,
};
use async_trait::async_trait;
use dashmap::DashMap;
use sea_orm::DatabaseConnection;
use tokio::sync::{Mutex, Notify, mpsc};
use tokio_util::sync::CancellationToken;

use crate::crud;
use crate::orchestrator::router::{NoopTaskRouter, TaskRouter};

/// Default interval for polling the queue when no notification arrives.
///
/// 10s is the backstop, *not* the dispatch latency target. With a real
/// [`crate::orchestrator::router::PostgresTaskRouter`] wired in, NOTIFY-driven wakes
/// land within milliseconds; this fallback only fires when a wake was
/// missed (listener disconnected, in-process Notify permit consumed
/// before a waiter parked, etc). Tightening it back to 1s would burn
/// ~1 DB query/sec per worker process at idle for no real-world gain
/// — most of the time we should be parked on `Notify::notified()`.
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Durable transport that persists task assignments in the database.
///
/// The coordinator inserts assignments; workers poll the table. A `Notify`
/// provides instant wake-up when a new task is enqueued, with a fallback
/// poll interval.
pub struct DurableTransport {
    db: DatabaseConnection,
    /// Unique identifier for this worker process.
    worker_id: String,

    /// Worker → Coordinator: events and outcomes (in-memory, ephemeral).
    message_tx: mpsc::Sender<WorkerMessage>,
    message_rx: Mutex<mpsc::Receiver<WorkerMessage>>,

    /// Wake signal: coordinator notifies when a new task is enqueued.
    new_task_notify: Notify,

    /// Per-task cancellation tokens, keyed by task_id.
    cancel_tokens: DashMap<String, CancellationToken>,

    /// Poll interval when no notification arrives.
    poll_interval: Duration,

    /// Optional task-tree scope. When set, [`recv_assignment`] only claims
    /// tasks whose `task_id` is exactly this value or starts with
    /// `"<value>."` (children/grandchildren by the
    /// `<parent>.<seq>` naming convention).
    ///
    /// The per-request `spawn_workflow_run_drive` path sets this to the
    /// run id so its worker doesn't poach tasks from a sibling run's
    /// coordinator — when that happens, the wrong coordinator receives the
    /// outcome (the task isn't in its in-memory `self.tasks` map),
    /// `handle_done` early-returns, and the right coordinator hangs
    /// waiting for an outcome that already arrived elsewhere.
    task_id_root: Option<String>,

    /// Cross-process wake source.
    ///
    /// In a single process, [`Self::new_task_notify`] handles wakes
    /// instantly — `assign()` calls `notify_one()` in the same address
    /// space. But assignments enqueued by a *different* app instance
    /// never touch our in-memory Notify; the only signal is the row
    /// landing in `agentic_task_queue`. The router (via Postgres
    /// LISTEN/NOTIFY in production, [`NoopTaskRouter`] in tests) is
    /// what makes those cross-process wakes fast — without it, workers
    /// would wait the full [`DEFAULT_POLL_INTERVAL`] before noticing.
    router: Arc<dyn TaskRouter>,
}

impl DurableTransport {
    /// Create a new durable transport.
    ///
    /// Uses [`NoopTaskRouter`] — workers wake only on in-process
    /// `assign()` or the 10s backstop poll. For production deployments
    /// with multi-instance scaling, use [`Self::with_router`] and pass
    /// a [`crate::orchestrator::router::PostgresTaskRouter`] so cross-instance
    /// enqueues wake workers within milliseconds.
    pub fn new(db: DatabaseConnection) -> Arc<Self> {
        Self::with_config(db, DEFAULT_POLL_INTERVAL)
    }

    /// Create with custom poll interval (useful for testing).
    pub fn with_config(db: DatabaseConnection, poll_interval: Duration) -> Arc<Self> {
        Self::with_config_and_root(db, poll_interval, None, Arc::new(NoopTaskRouter))
    }

    /// Create a transport whose worker only sees tasks under the given root.
    ///
    /// `root_task_id` typically equals the run id of the workflow this
    /// transport's coordinator owns. Children spawned by that coordinator
    /// follow the `<root>.<n>` naming convention and are also claimed by
    /// this transport's worker; tasks belonging to other runs are skipped.
    pub fn new_scoped(db: DatabaseConnection, root_task_id: String) -> Arc<Self> {
        Self::with_config_and_root(
            db,
            DEFAULT_POLL_INTERVAL,
            Some(root_task_id),
            Arc::new(NoopTaskRouter),
        )
    }

    /// Create a transport that uses the given router for cross-process
    /// wake notifications.
    ///
    /// Production callers: pass a process-shared
    /// [`crate::orchestrator::router::PostgresTaskRouter`] so every workflow run on
    /// this instance benefits from the same LISTEN connection rather
    /// than opening one per run. The router is cheap to clone (it's
    /// just an `Arc` to the shared listener state).
    pub fn with_router(
        db: DatabaseConnection,
        router: Arc<dyn TaskRouter>,
        root_task_id: Option<String>,
    ) -> Arc<Self> {
        Self::with_config_and_root(db, DEFAULT_POLL_INTERVAL, root_task_id, router)
    }

    fn with_config_and_root(
        db: DatabaseConnection,
        poll_interval: Duration,
        task_id_root: Option<String>,
        router: Arc<dyn TaskRouter>,
    ) -> Arc<Self> {
        let (message_tx, message_rx) = mpsc::channel(1024);
        let worker_id = format!("worker-{}", uuid::Uuid::new_v4());
        Arc::new(Self {
            db,
            worker_id,
            message_tx,
            message_rx: Mutex::new(message_rx),
            new_task_notify: Notify::new(),
            cancel_tokens: DashMap::new(),
            poll_interval,
            task_id_root,
            router,
        })
    }

    /// Wake any polling workers so they check the queue immediately.
    ///
    /// Used by recovery after re-queuing tasks — the normal `assign()` path
    /// calls this internally, but `requeue_task()` bypasses the transport.
    pub fn notify_new_task(&self) {
        self.new_task_notify.notify_waiters();
    }

    /// Update the heartbeat for a claimed task.
    ///
    /// Workers should call this periodically while executing a task to prevent
    /// the reaper from re-queuing it.
    pub async fn heartbeat(&self, task_id: &str) -> Result<(), TransportError> {
        crud::update_queue_heartbeat(&self.db, task_id)
            .await
            .map_err(|e| TransportError::Other(format!("heartbeat failed: {e}")))
    }

    /// Run a single reaper cycle: re-queue stale tasks, dead-letter exhausted ones.
    ///
    /// Returns the number of tasks affected.
    pub async fn run_reaper(&self) -> u64 {
        match crud::reap_stale_tasks(&self.db).await {
            Ok(count) => {
                if count > 0 {
                    tracing::info!(count, "reaper: re-queued or dead-lettered stale tasks");
                    // Wake workers so they can pick up re-queued tasks.
                    self.new_task_notify.notify_waiters();
                }
                count
            }
            Err(e) => {
                tracing::error!("reaper failed: {e}");
                0
            }
        }
    }

    // `spawn_reaper` removed: live reaping is now the responsibility of
    // [`crate::orchestrator::background::start`], which runs a single process-level
    // task per app instance. The old per-transport spawn was dead code
    // (no production caller) — see commit history for the audit trail.

    /// Run a single stuck-workflow-run sweep.
    ///
    /// Finds workflow runs in non-terminal `task_status` with no queue entry
    /// for themselves or any descendant, and re-enqueues a fresh
    /// `WorkflowDecision` for each. The decider is idempotent under the
    /// `decision_version` CAS, so a race where two sweepers (or a sweeper and
    /// a real worker) both re-enqueue is safe — one will win the CAS, the
    /// other will return `VersionConflict` and exit cleanly.
    ///
    /// `grace_secs` is the minimum `updated_at` age before a run is eligible,
    /// to avoid racing with an in-flight commit. Returns the number of runs
    /// rescued in this pass.
    pub async fn run_stuck_run_sweeper(&self, grace_secs: u64) -> u64 {
        let stuck = match crud::find_stuck_workflow_runs(&self.db, grace_secs).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("stuck-run sweeper: query failed: {e}");
                return 0;
            }
        };

        let mut rescued: u64 = 0;
        for run in &stuck {
            let spec = TaskSpec::WorkflowDecision {
                run_id: run.run_id.clone(),
                pending_child_answer: None,
            };
            // Re-enqueue as a WorkflowDecision. `enqueue_task` upserts on
            // conflict — if another writer has already re-driven the run
            // between our query and this call, we harmlessly overwrite with
            // the same spec shape.
            if let Err(e) =
                crud::enqueue_task(&self.db, &run.run_id, &run.run_id, None, &spec, None).await
            {
                tracing::error!(
                    run_id = %run.run_id,
                    error = %e,
                    "stuck-run sweeper: failed to re-enqueue WorkflowDecision"
                );
                continue;
            }
            tracing::warn!(
                run_id = %run.run_id,
                task_status = ?run.task_status,
                "stuck-run sweeper: re-enqueued WorkflowDecision"
            );
            rescued += 1;
        }

        if rescued > 0 {
            self.new_task_notify.notify_waiters();
        }
        rescued
    }

    /// Spawn a background sweeper that periodically calls
    /// [`run_stuck_run_sweeper`](Self::run_stuck_run_sweeper).
    ///
    /// Use `grace_secs >= interval` so the sweeper never acts on a run it
    /// just observed in the previous pass.
    pub fn spawn_stuck_run_sweeper(
        self: &Arc<Self>,
        interval: Duration,
        grace_secs: u64,
    ) -> CancellationToken {
        let cancel = CancellationToken::new();
        let transport = Arc::clone(self);
        let cancel_clone = cancel.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await; // first tick is immediate, skip it
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        transport.run_stuck_run_sweeper(grace_secs).await;
                    }
                    _ = cancel_clone.cancelled() => break,
                }
            }
        });
        cancel
    }

    /// Spawn a heartbeat loop for a specific task.
    ///
    /// Returns a `CancellationToken` — cancel it when the task completes.
    pub fn spawn_heartbeat(
        self: &Arc<Self>,
        task_id: String,
        interval: Duration,
    ) -> CancellationToken {
        let cancel = CancellationToken::new();
        let transport = Arc::clone(self);
        let cancel_clone = cancel.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if let Err(e) = transport.heartbeat(&task_id).await {
                            tracing::warn!(task_id = %task_id, "heartbeat failed: {e}");
                            break;
                        }
                    }
                    _ = cancel_clone.cancelled() => break,
                }
            }
        });
        cancel
    }
}

#[async_trait]
impl CoordinatorTransport for DurableTransport {
    async fn assign(&self, assignment: TaskAssignment) -> Result<(), TransportError> {
        // Register a cancellation token for this task.
        self.cancel_tokens
            .insert(assignment.task_id.clone(), CancellationToken::new());

        // Persist the assignment in the queue.
        crud::enqueue_task(
            &self.db,
            &assignment.task_id,
            &assignment.run_id,
            assignment.parent_task_id.as_deref(),
            &assignment.spec,
            assignment.policy.as_ref(),
        )
        .await
        .map_err(|e| TransportError::Other(format!("enqueue failed: {e}")))?;

        // Wake any polling worker immediately.
        self.new_task_notify.notify_one();

        Ok(())
    }

    async fn recv(&self) -> Option<WorkerMessage> {
        self.message_rx.lock().await.recv().await
    }

    async fn cancel(&self, task_id: &str) -> Result<(), TransportError> {
        // Update queue status in DB.
        crud::cancel_queued_task(&self.db, task_id)
            .await
            .map_err(|e| TransportError::Other(format!("cancel failed: {e}")))?;

        // Also fire the in-memory cancellation token for already-running tasks.
        if let Some(token) = self.cancel_tokens.get(task_id) {
            token.cancel();
        }

        Ok(())
    }

    async fn cancel_subtree(&self, root_task_id: &str) -> Result<(), TransportError> {
        // Cancel the root's queue entry + token.
        self.cancel(root_task_id).await?;

        // Fire tokens for every descendant. Child ids are formatted as
        // `{parent_id}.{counter}` by `Coordinator::handle_suspended`, so
        // every descendant's task_id starts with `"{root_task_id}."`.
        let prefix = format!("{root_task_id}.");
        for entry in self.cancel_tokens.iter() {
            if entry.key().starts_with(&prefix) {
                entry.value().cancel();
            }
        }

        Ok(())
    }
}

#[async_trait]
impl WorkerTransport for DurableTransport {
    async fn recv_assignment(&self) -> Option<TaskAssignment> {
        loop {
            // Try to claim a task from the queue. Scoped transports only
            // see tasks under their root — see `task_id_root` for why.
            let claim_result = match &self.task_id_root {
                Some(root) => crud::claim_task_under_root(&self.db, &self.worker_id, root).await,
                None => crud::claim_task(&self.db, &self.worker_id).await,
            };
            match claim_result {
                Ok(Some(entry)) => {
                    // Deserialize spec and policy back into the assignment.
                    let spec: TaskSpec = match serde_json::from_value(entry.spec) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!(task_id = %entry.task_id, "failed to deserialize task spec: {e}");
                            // Mark as failed and try the next task.
                            let _ = crud::fail_queue_task(&self.db, &entry.task_id).await;
                            continue;
                        }
                    };
                    let policy = entry.policy.and_then(|p| {
                        serde_json::from_value(p)
                            .map_err(|e| {
                                tracing::warn!(task_id = %entry.task_id, "failed to deserialize task policy: {e}");
                            })
                            .ok()
                    });

                    return Some(TaskAssignment {
                        task_id: entry.task_id,
                        parent_task_id: entry.parent_task_id,
                        run_id: entry.run_id,
                        spec,
                        policy,
                    });
                }
                Ok(None) => {
                    // No tasks available. Wait for any wake signal:
                    //   - `new_task_notify`: same-process `assign()` call
                    //   - `router.wait_for_task`: cross-process LISTEN wake
                    //     (or its own internal backstop timeout)
                    //
                    // The router's `wait_for_task` already enforces the
                    // backstop via its `timeout` arg, so we don't need
                    // a separate `tokio::time::sleep` here — that would
                    // double up the timeout and waste a poll-interval's
                    // worth of latency on every loop iteration.
                    tokio::select! {
                        _ = self.new_task_notify.notified() => {}
                        _ = self.router.wait_for_task(&[], self.poll_interval) => {}
                    }
                }
                Err(e) => {
                    tracing::error!("failed to claim task from queue: {e}");
                    tokio::time::sleep(self.poll_interval).await;
                }
            }
        }
    }

    async fn send(&self, msg: WorkerMessage) -> Result<(), TransportError> {
        // On terminal outcomes, update the queue entry.
        match &msg {
            WorkerMessage::Outcome { task_id, outcome } => {
                let result = match outcome {
                    TaskOutcome::Done { .. } => crud::complete_queue_task(&self.db, task_id).await,
                    TaskOutcome::Failed(_) => crud::fail_queue_task(&self.db, task_id).await,
                    TaskOutcome::Cancelled => crud::cancel_queued_task(&self.db, task_id).await,
                    // Suspended is not terminal — task may resume.
                    TaskOutcome::Suspended { .. } => Ok(()),
                };
                if let Err(e) = result {
                    tracing::warn!(task_id, "failed to update queue status: {e}");
                }
            }
            WorkerMessage::Event { .. } => {
                // Events don't affect queue status.
            }
        }

        // Forward to coordinator via in-memory channel.
        self.message_tx
            .send(msg)
            .await
            .map_err(|_| TransportError::ChannelClosed)
    }

    fn cancellation_token(&self, task_id: &str) -> CancellationToken {
        self.cancel_tokens
            .entry(task_id.to_string())
            .or_default()
            .clone()
    }

    async fn heartbeat(&self, task_id: &str) -> Result<(), TransportError> {
        crud::update_queue_heartbeat(&self.db, task_id)
            .await
            .map_err(|e| TransportError::Other(format!("heartbeat failed: {e}")))
    }

    fn spawn_heartbeat(&self, task_id: &str, interval: Duration) -> CancellationToken {
        let cancel = CancellationToken::new();
        let db = self.db.clone();
        let task_id = task_id.to_string();
        let cancel_clone = cancel.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if let Err(e) = crud::update_queue_heartbeat(&db, &task_id).await {
                            tracing::warn!(task_id = %task_id, "heartbeat failed: {e}");
                            break;
                        }
                    }
                    _ = cancel_clone.cancelled() => break,
                }
            }
        });
        cancel
    }
}
