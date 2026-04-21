//! Coordinator: manages a task tree, routes outcomes, spawns child tasks.
//!
//! The coordinator is the central orchestration point. It receives
//! [`agentic_core::transport::WorkerMessage`]s from the transport and decides
//! what to do:
//!
//! - **Events** are persisted to the DB and forwarded to SSE subscribers.
//!   Child task events are also injected into the parent's event stream.
//! - **Outcomes** trigger the appropriate next action: mark done, resume
//!   parent, wait for human input, or spawn a child task for delegation.
//!
//! Implementation is split across sibling modules by concern:
//! - [`recovery`]: rebuilding the task tree from persisted state on restart.
//! - [`run_loop`]: the main select-loop and suspend-timeout enforcement.
//! - [`outcomes`]: handlers for `WorkerMessage::Event` and `TaskOutcome` variants.
//! - [`fanout`]: per-parent child-result accumulation and resume logic.
//! - [`suspension`]: handlers for `Suspended` outcomes and human answers.
//! - [`retry`]: retry/fallback decisions + delegation event emitters.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use agentic_core::delegation::{
    FanoutFailurePolicy, TaskAssignment, TaskOutcome, TaskPolicy, TaskSpec,
};
use agentic_core::transport::CoordinatorTransport;
use sea_orm::DatabaseConnection;
use tokio::sync::mpsc;

use crate::state::RuntimeState;

pub mod fanout;
pub mod outcomes;
pub mod recovery;
pub mod retry;
pub mod run_loop;
pub mod suspension;

pub use recovery::PendingResume;

// ── Defaults ────────────────────────────────────────────────────────────────

const DEFAULT_SUSPEND_TIMEOUT: Duration = Duration::from_secs(30 * 60); // 30 min
const DEFAULT_DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

// ── TaskNode ────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub(super) struct TaskNode {
    pub(super) run_id: String,
    pub(super) parent_task_id: Option<String>,
    pub(super) status: TaskStatus,
    /// Stored when the task suspends for delegation, consumed on resume.
    pub(super) suspend_data: Option<agentic_core::human_input::SuspendedRunData>,
    /// Event sequence counter for the run.
    pub(super) next_seq: i64,
    /// When the task entered a suspended state (for timeout enforcement).
    pub(super) suspended_at: Option<tokio::time::Instant>,
    // ── Retry/fallback state ────────────────────────────────────────────
    /// The original TaskSpec for this child task (needed for retries).
    pub(super) original_spec: Option<TaskSpec>,
    /// The policy governing retries and fallbacks.
    pub(super) policy: Option<TaskPolicy>,
    /// Current retry attempt (0 = initial attempt).
    pub(super) attempt: u32,
    /// Index into `policy.fallback_targets` (0 = still on primary).
    pub(super) fallback_index: usize,
}

#[derive(Debug)]
pub(super) enum TaskStatus {
    Running,
    SuspendedHuman,
    WaitingOnChildren {
        child_task_ids: Vec<String>,
        completed: HashMap<String, ChildResult>,
        failure_policy: FanoutFailurePolicy,
    },
    Done,
    Failed,
}

/// Result of a completed child task in a fan-out.
#[derive(Debug, Clone)]
pub(super) enum ChildResult {
    Done(String),
    Failed(String),
}

// ── Coordinator ─────────────────────────────────────────────────────────────

/// Manages a tree of tasks, routing outcomes between parents and children.
pub struct Coordinator {
    pub(super) db: DatabaseConnection,
    pub(super) state: Arc<RuntimeState>,
    pub(super) transport: Arc<dyn CoordinatorTransport>,
    pub(super) tasks: HashMap<String, TaskNode>,
    /// Counter for generating child task IDs.
    pub(super) child_counter: u64,
    /// Recovery attempt number (from DB). 0 = original run, incremented on
    /// each recovery. Used in child ID generation to avoid PK collisions.
    pub(super) attempt: i32,
    /// Channel for receiving human answers from the HTTP layer.
    /// Maps run_id → receiver.
    pub(super) answer_rxs: HashMap<String, mpsc::Receiver<String>>,
    /// How long a task can stay suspended before being auto-failed.
    pub(super) suspend_timeout: Duration,
    /// How long to wait for late events during the drain phase.
    pub(super) drain_timeout: Duration,
}

impl Coordinator {
    pub fn new(
        db: DatabaseConnection,
        state: Arc<RuntimeState>,
        transport: Arc<dyn CoordinatorTransport>,
    ) -> Self {
        Self {
            db,
            state,
            transport,
            tasks: HashMap::new(),
            child_counter: 0,
            attempt: 0,
            answer_rxs: HashMap::new(),
            suspend_timeout: DEFAULT_SUSPEND_TIMEOUT,
            drain_timeout: DEFAULT_DRAIN_TIMEOUT,
        }
    }

    /// Set the timeout for suspended tasks (human input or delegation).
    pub fn with_suspend_timeout(mut self, timeout: Duration) -> Self {
        self.suspend_timeout = timeout;
        self
    }

    /// Set the timeout for draining late events after the main loop exits.
    pub fn with_drain_timeout(mut self, timeout: Duration) -> Self {
        self.drain_timeout = timeout;
        self
    }

    /// Register a human-answer channel for a run (called by the HTTP layer).
    pub fn register_answer_channel(&mut self, run_id: String, rx: mpsc::Receiver<String>) {
        self.answer_rxs.insert(run_id, rx);
    }

    /// Submit a root task for execution by a worker.
    pub async fn submit_root(
        &mut self,
        run_id: String,
        spec: TaskSpec,
    ) -> Result<(), crate::state::RunError> {
        self.register_root(run_id.clone(), 0);

        let assignment = TaskAssignment {
            task_id: run_id.clone(),
            parent_task_id: None,
            run_id,
            spec,
            policy: None,
        };

        self.transport
            .assign(assignment)
            .await
            .map_err(|e| crate::state::RunError::Db(sea_orm::DbErr::Custom(e.to_string())))
    }

    /// Register a root task that is already running externally.
    ///
    /// Use this when the pipeline was started outside the coordinator (e.g.,
    /// via `PipelineBuilder::start()`) and its events/outcomes are being
    /// forwarded to the coordinator's transport by a virtual worker.
    /// Unlike [`submit_root`], this does NOT publish an assignment.
    ///
    /// `next_seq` should be 0 for fresh runs or `max_existing_seq + 1` for
    /// cold-resumed runs to avoid event seq conflicts.
    pub fn register_root(&mut self, run_id: String, next_seq: i64) {
        let task_id = run_id.clone();
        self.tasks.insert(
            task_id,
            TaskNode {
                run_id,
                parent_task_id: None,
                status: TaskStatus::Running,
                suspend_data: None,
                next_seq,
                suspended_at: None,
                original_spec: None,
                policy: None,
                attempt: 0,
                fallback_index: 0,
            },
        );
    }

    /// Process pending resumes from crash recovery. Call this after `from_db`
    /// to resume parents that were waiting on children that completed before
    /// the crash.
    pub async fn process_pending_resumes(&mut self, resumes: Vec<PendingResume>) {
        for resume in resumes {
            tracing::info!(
                target: "coordinator",
                parent_id = %resume.parent_task_id,
                answer_len = resume.answer.len(),
                "resuming parent from crash recovery"
            );
            self.resume_parent(&resume.parent_task_id, resume.answer)
                .await;
        }
    }
}

// ── Loop / retry helper enums ───────────────────────────────────────────────

pub(super) enum LoopAction {
    WorkerEvent {
        task_id: String,
        event_type: String,
        payload: serde_json::Value,
    },
    WorkerOutcome {
        task_id: String,
        outcome: TaskOutcome,
    },
    HumanAnswer {
        task_id: String,
        answer: String,
    },
    TransportClosed,
    /// A suspend timeout expired — loop back to check_suspend_timeouts.
    SuspendTimeout,
}

/// What to do when a child task fails and has a retry/fallback policy.
pub(super) enum RetryAction {
    /// Retry the same spec after a backoff delay.
    Retry {
        delay: Duration,
        attempt: u32,
        spec: TaskSpec,
        run_id: String,
        parent_task_id: Option<String>,
    },
    /// Try a fallback target.
    Fallback {
        new_spec: TaskSpec,
        fallback_index: usize,
        run_id: String,
        parent_task_id: Option<String>,
    },
}

/// Derive the `source_type` for a child run from its `TaskSpec`.
pub(super) fn source_type_for_spec(spec: &TaskSpec) -> &'static str {
    match spec {
        TaskSpec::Agent { agent_id, .. } => {
            if agent_id == "__builder__" {
                "builder"
            } else {
                "analytics"
            }
        }
        TaskSpec::Workflow { .. } => "workflow",
        TaskSpec::WorkflowStep { .. } => "workflow_step",
        TaskSpec::WorkflowDecision { .. } => "workflow",
        TaskSpec::Resume { .. } => "analytics", // resume inherits parent type
    }
}
