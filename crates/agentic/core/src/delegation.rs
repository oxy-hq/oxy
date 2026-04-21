//! Types for cross-agent delegation and task coordination.
//!
//! These types are used by the coordinator to manage a tree of tasks where
//! agents and workflows can delegate work to each other via the suspend/resume
//! mechanism.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::events::HumanInputQuestion;
use crate::human_input::SuspendedRunData;

// ── SuspendReason ────────────────────────────────────────────────────────────

/// Why a pipeline suspended.
///
/// Carried by [`BackTarget::Suspend`] and [`PipelineOutcome::Suspended`] to
/// tell the coordinator how to fulfil the suspension: either present questions
/// to a human or spawn a child task.
///
/// [`BackTarget::Suspend`]: crate::back_target::BackTarget::Suspend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SuspendReason {
    /// The LLM invoked `ask_user` — a human must answer.
    HumanInput { questions: Vec<HumanInputQuestion> },
    /// The solver requested delegation to another agent or workflow.
    Delegation {
        target: DelegationTarget,
        /// The question/instruction for the delegate.
        request: String,
        /// Opaque context the coordinator may forward to the child task.
        context: Value,
        /// Optional retry/fallback policy for the delegated task.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        policy: Option<TaskPolicy>,
    },
    /// The solver requested parallel delegation to multiple targets.
    ParallelDelegation {
        targets: Vec<DelegationItem>,
        /// How to handle partial failures.
        failure_policy: FanoutFailurePolicy,
    },
}

// ── Parallel delegation types ───────────────────────────────────────────────

/// A single delegation target within a parallel fan-out.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationItem {
    pub target: DelegationTarget,
    /// The question/instruction for the delegate.
    pub request: String,
    /// Opaque context forwarded to the child task.
    #[serde(default)]
    pub context: Value,
}

/// How the coordinator handles partial failures in a parallel delegation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FanoutFailurePolicy {
    /// Fail the parent immediately when any child fails; cancel remaining siblings.
    #[default]
    FailFast,
    /// Wait for all children to complete; parent receives partial results.
    BestEffort,
}

// ── Task policies ───────────────────────────────────────────────────────────

/// Retry and fallback configuration for a delegated task.
///
/// Attached to [`TaskAssignment`] and enforced transparently by the coordinator.
/// Domain crates opt in by populating this field; `None` means no retry.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskPolicy {
    /// Retry the same target on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    /// Fallback targets to try if the primary (and all retries) fail.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallback_targets: Vec<DelegationTarget>,
}

/// How to retry a failed task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts (not counting the initial attempt).
    pub max_retries: u32,
    /// Backoff between retries.
    pub backoff: BackoffStrategy,
    /// Only retry on failures matching these patterns (empty = retry all).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retry_on: Vec<String>,
}

/// Backoff strategy between retry attempts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackoffStrategy {
    /// Fixed delay between retries.
    Fixed { delay_ms: u64 },
    /// Exponential backoff: `initial_delay_ms * 2^attempt`, capped at `max_delay_ms`.
    Exponential {
        initial_delay_ms: u64,
        max_delay_ms: u64,
    },
}

impl BackoffStrategy {
    /// Compute the delay for the given attempt (0-indexed).
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        match self {
            BackoffStrategy::Fixed { delay_ms } => Duration::from_millis(*delay_ms),
            BackoffStrategy::Exponential {
                initial_delay_ms,
                max_delay_ms,
            } => {
                let delay = initial_delay_ms.saturating_mul(2u64.saturating_pow(attempt));
                Duration::from_millis(delay.min(*max_delay_ms))
            }
        }
    }
}

// ── DelegationTarget ─────────────────────────────────────────────────────────

/// What to delegate to.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DelegationTarget {
    /// Another agentic agent (analytics, builder, etc.).
    Agent { agent_id: String },
    /// A workflow/procedure file.
    Workflow { workflow_ref: String },
}

// ── TaskSpec ─────────────────────────────────────────────────────────────────

/// Describes a unit of work the coordinator assigns to a worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskSpec {
    /// Start a fresh agent run.
    Agent { agent_id: String, question: String },
    /// Execute a workflow/procedure.
    Workflow {
        workflow_ref: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        variables: Option<Value>,
    },
    /// Resume a suspended run with an answer.
    Resume {
        run_id: String,
        resume_data: SuspendedRunData,
        answer: String,
    },
    /// Execute a single workflow step (SQL, semantic query, etc.).
    ///
    /// The step worker deserializes the config, builds a renderer from the
    /// render context, executes the step, and returns the `OutputContainer`
    /// as the answer string.
    WorkflowStep {
        /// Serialized step config (the Task YAML parsed into JSON).
        step_config: Value,
        /// Accumulated render context from prior steps (`{{ step_name.field }}`).
        render_context: Value,
        /// Workflow-level config (workspace path, database configs, globals).
        workflow_context: Value,
    },
    /// Stateless "decision task" for a workflow (Temporal-inspired).
    ///
    /// The worker loads the workflow state snapshot (from the workflow domain's
    /// extension table), folds any `pending_child_answer` into the state, runs
    /// the pure `WorkflowDecider::decide()` function to compute the next action,
    /// persists the new state, and exits. No in-memory channels span decision
    /// task boundaries — everything is in the DB.
    WorkflowDecision {
        /// The workflow run_id (also the PK of the workflow-state table).
        run_id: String,
        /// Latest child completion to fold into state before deciding.
        /// `None` on the initial decision (workflow just started) and on
        /// inline-chain decisions (an inline step produced an output but the
        /// workflow isn't done — chain into the next decision).
        #[serde(skip_serializing_if = "Option::is_none")]
        pending_child_answer: Option<ChildCompletion>,
    },
}

/// A completed child task's outcome, packaged for folding into a workflow
/// decision's input state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildCompletion {
    /// The child task_id (e.g. `"<run_id>.3"`).
    pub child_task_id: String,
    /// Which workflow step this child was spawned for.
    pub step_index: usize,
    /// The step's name from the workflow config.
    pub step_name: String,
    /// `"done"` | `"failed"` | `"cancelled"` | `"timed_out"`.
    pub status: String,
    /// The child's answer (for done) or error message (for failed).
    pub answer: String,
}

// ── TaskAssignment ───────────────────────────────────────────────────────────

/// A task assigned by the coordinator to a worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssignment {
    /// Unique identifier for this task.
    pub task_id: String,
    /// If this is a child task, the parent's task_id.
    pub parent_task_id: Option<String>,
    /// The run_id to use for DB persistence (coordinator assigns).
    pub run_id: String,
    /// What to do.
    pub spec: TaskSpec,
    /// Optional retry/fallback policy enforced by the coordinator.
    pub policy: Option<TaskPolicy>,
}

// ── TaskOutcome ──────────────────────────────────────────────────────────────

/// Outcome of a task, reported by the worker back to the coordinator.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskOutcome {
    /// Task completed with an answer.
    Done {
        answer: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<Value>,
    },
    /// Task suspended — coordinator decides how to fulfil it.
    Suspended {
        reason: SuspendReason,
        resume_data: SuspendedRunData,
        trace_id: String,
    },
    /// Task failed.
    Failed(String),
    /// Task was cancelled.
    Cancelled,
}
