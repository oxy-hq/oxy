//! Orchestrator: drives the FSM pipeline for a given domain.
//!
//! Implementation is split across sibling modules by concern:
//! - [`transitions`]: [`TransitionResult`], [`StateHandler`], internal [`PipelineResult`].
//! - [`fanout`]: [`run_fanout`] and its concurrent/serial execution helpers.
//! - [`handlers`]: [`build_default_handlers`] — the default FSM wiring.
//! - [`run_loop`]: the main [`Orchestrator`] struct plus `run_pipeline_inner`.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::delegation::SuspendReason;
use crate::domain::Domain;
use crate::events::{CoreEvent, DomainEvents, Event, EventStream};
use crate::human_input::SuspendedRunData;
use crate::state::ProblemState;

pub mod api;
pub mod fanout;
pub mod handlers;
pub mod run_loop;
pub mod transitions;

pub use api::Orchestrator;
pub use fanout::run_fanout;
pub use handlers::build_default_handlers;
pub use transitions::{StateHandler, TransitionResult};

// ── RunContext ────────────────────────────────────────────────────────────────

/// Accumulated outputs from prior stages, owned by the orchestrator.
///
/// Passed as a read-only reference to each [`StateHandler::execute`] closure
/// so that handlers can access the outputs of earlier stages without storing
/// them on the solver.  This enforces the "orchestrator owns context" principle.
///
/// Fields are populated by the orchestrator loop after each successful state
/// transition: `intent` after Clarifying, `spec` after Specifying.
#[derive(Clone)]
pub struct RunContext<D: Domain> {
    /// Clarified intent produced by the Clarifying stage.
    pub intent: Option<D::Intent>,
    /// Resolved spec produced by the Specifying stage.
    pub spec: Option<D::Spec>,
    /// Retry context from the most recent back-edge, if any.
    ///
    /// Set by the orchestrator when a `Diagnosing` state resolves to a
    /// recovery state.  Cleared after each successful forward transition.
    /// Handlers pass it to prompt builders so the LLM sees prior errors.
    pub retry_ctx: Option<crate::back_target::RetryContext>,
}

// ── CompletedTurn / SessionMemory ─────────────────────────────────────────────

/// A structured record of one completed pipeline run.
///
/// Stored by the orchestrator after each successful [`Orchestrator::run`] call.
/// The solver uses these to inject prior context into LLM prompts for
/// multi-turn conversations.
#[derive(Clone)]
pub struct CompletedTurn<D: Domain> {
    /// The clarified intent from this turn.
    pub intent: D::Intent,
    /// The resolved spec (if the pipeline reached Specifying).
    pub spec: Option<D::Spec>,
    /// The natural-language answer returned to the user.
    pub answer: D::Answer,
    /// The trace ID for this run (links to events).
    pub trace_id: String,
}

/// Accumulated history from prior completed runs in this session.
///
/// Owned by the orchestrator and passed as a read-only reference to
/// [`StateHandler::execute`] closures so prompt builders can incorporate
/// prior context into LLM calls.
#[derive(Clone)]
pub struct SessionMemory<D: Domain> {
    turns: Vec<CompletedTurn<D>>,
    /// Maximum number of prior turns to retain. Oldest are dropped first.
    max_turns: usize,
}

impl<D: Domain> SessionMemory<D> {
    /// Create a new empty session memory with the given capacity cap.
    pub fn new(max_turns: usize) -> Self {
        Self {
            turns: Vec::new(),
            max_turns,
        }
    }

    /// Push a completed turn, evicting the oldest if at capacity.
    pub fn push(&mut self, turn: CompletedTurn<D>) {
        if self.turns.len() >= self.max_turns {
            self.turns.remove(0);
        }
        self.turns.push(turn);
    }

    /// Read-only slice of all retained turns, oldest first.
    pub fn turns(&self) -> &[CompletedTurn<D>] {
        &self.turns
    }

    /// Create a shallow clone suitable for concurrent fan-out tasks.
    ///
    /// This avoids the `D: Clone` bound that `#[derive(Clone)]` imposes
    /// on the type parameter itself.
    pub fn clone_shallow(&self) -> Self {
        let turns = self
            .turns
            .iter()
            .map(|t| CompletedTurn {
                intent: t.intent.clone(),
                spec: t.spec.clone(),
                answer: t.answer.clone(),
                trace_id: t.trace_id.clone(),
            })
            .collect();
        Self {
            turns,
            max_turns: self.max_turns,
        }
    }

    /// `true` when no turns have been stored yet.
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }

    /// Number of retained turns.
    pub fn len(&self) -> usize {
        self.turns.len()
    }

    /// Clear all retained history (e.g. when the user starts a new topic).
    pub fn clear(&mut self) {
        self.turns.clear();
    }
}

impl<D: Domain> Default for SessionMemory<D> {
    fn default() -> Self {
        Self::new(10)
    }
}

// ── PipelineOutput ────────────────────────────────────────────────────────────

/// The raw output of a single FSM pipeline execution.
///
/// Returned by [`Orchestrator::run_pipeline`] — the caller decides what to
/// record in session memory.  This separation lets scatter-gather execute
/// N sub-pipelines and push one combined [`CompletedTurn`] instead of N
/// intermediate turns.
pub struct PipelineOutput<D: Domain> {
    /// The answer produced by the Interpret stage.
    pub answer: D::Answer,
    /// The clarified intent (set during Clarifying → Specifying transition).
    pub intent: D::Intent,
    /// The resolved spec, if the pipeline reached Specifying.
    pub spec: Option<D::Spec>,
}

// ── Trace-ID generator ────────────────────────────────────────────────────────

static TRACE_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generate a fresh, monotonically-increasing trace ID for a new run.
///
/// Returns `"trace-N"` where N is a process-wide counter.
pub fn next_trace_id() -> String {
    format!("trace-{}", TRACE_COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Generate a child trace ID for fan-out sub-specs.
///
/// Returns `"{parent}.{index}"` — e.g. `"trace-42.1"` for the second
/// sub-spec of parent `trace-42`.
pub fn child_trace_id(parent: &str, index: usize) -> String {
    format!("{parent}.{index}")
}

// ── State name helpers ────────────────────────────────────────────────────────

pub(super) fn state_name<D: Domain>(state: &ProblemState<D>) -> &'static str {
    match state {
        ProblemState::Clarifying(_) => "clarifying",
        ProblemState::Specifying(_) => "specifying",
        ProblemState::Solving(_) => "solving",
        ProblemState::Executing(_) => "executing",
        ProblemState::Interpreting(_) => "interpreting",
        ProblemState::Diagnosing { .. } => "diagnosing",
        ProblemState::Done(_) => "done",
    }
}

/// Return the ordinal position of a pipeline stage name.
///
/// Used by the Diagnosing arm to distinguish forward transitions
/// (e.g. executing → interpreting via `ValueAnomaly` pass-through)
/// from genuine back-edges (e.g. executing → solving).
pub(super) fn stage_order(name: &str) -> u8 {
    match name {
        "clarifying" => 0,
        "specifying" => 1,
        "solving" => 2,
        "executing" => 3,
        "interpreting" => 4,
        "done" => 5,
        _ => u8::MAX,
    }
}

// ── Error type ────────────────────────────────────────────────────────────────

/// Error returned by [`Orchestrator::run`].
pub enum OrchestratorError<D: Domain> {
    /// The pipeline exceeded the configured maximum number of state
    /// transitions, most likely due to an unresolvable back-edge cycle.
    MaxIterationsExceeded,

    /// A fatal domain error was propagated from [`DomainSolver::diagnose`]
    /// or from a [`StateHandler::diagnose`] that returned `None`.
    ///
    /// [`DomainSolver::diagnose`]: crate::solver::DomainSolver::diagnose
    Fatal(D::Error),

    /// [`Orchestrator::resume`] was called on a solver that does not support
    /// HITL resumption (i.e. [`DomainSolver::problem_state_from_resume`]
    /// returned `None`).
    ///
    /// [`DomainSolver::problem_state_from_resume`]: crate::solver::DomainSolver::problem_state_from_resume
    /// [`Orchestrator::resume`]: crate::orchestrator::Orchestrator::resume
    ResumeNotSupported,

    /// The pipeline suspended — either for human input or agent delegation.
    ///
    /// The caller should inspect `reason` to decide how to fulfil:
    /// - [`SuspendReason::HumanInput`]: present questions to the user.
    /// - [`SuspendReason::Delegation`]: spawn a child task.
    ///
    /// In both cases, call [`Orchestrator::resume`] with the stored data and
    /// the answer when ready.
    ///
    /// [`SuspendReason::HumanInput`]: crate::delegation::SuspendReason::HumanInput
    /// [`SuspendReason::Delegation`]: crate::delegation::SuspendReason::Delegation
    /// [`Orchestrator::resume`]: crate::orchestrator::Orchestrator::resume
    Suspended {
        /// Why the pipeline suspended.
        reason: SuspendReason,
        /// Minimal payload needed to resume the pipeline.
        resume_data: SuspendedRunData,
        /// Trace ID for this run.
        trace_id: String,
    },
}

impl<D: Domain> PartialEq for OrchestratorError<D>
where
    D::Error: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::MaxIterationsExceeded, Self::MaxIterationsExceeded) => true,
            (Self::Fatal(a), Self::Fatal(b)) => a == b,
            (Self::Suspended { trace_id: a, .. }, Self::Suspended { trace_id: b, .. }) => a == b,
            _ => false,
        }
    }
}

impl<D: Domain> std::fmt::Debug for OrchestratorError<D>
where
    D::Error: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MaxIterationsExceeded => write!(f, "MaxIterationsExceeded"),
            Self::ResumeNotSupported => write!(f, "ResumeNotSupported"),
            Self::Fatal(e) => write!(f, "Fatal({e:?})"),
            Self::Suspended { trace_id, .. } => {
                write!(f, "Suspended {{ trace_id: {trace_id:?} }}")
            }
        }
    }
}

// ── Emit helper ───────────────────────────────────────────────────────────────

/// Send a core event on the channel, ignoring send errors (e.g. closed receiver).
pub(super) async fn emit<Ev: DomainEvents>(tx: &Option<EventStream<Ev>>, event: CoreEvent) {
    if let Some(tx) = tx {
        let _ = tx.send(Event::Core(event)).await;
    }
}
