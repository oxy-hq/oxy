//! [`TransitionResult`], [`StateHandler`], and the internal [`PipelineResult`].

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::domain::Domain;
use crate::events::{DomainEvents, EventStream};
use crate::solver::DomainSolver;
use crate::state::ProblemState;

use super::{PipelineOutput, RunContext, SessionMemory};

// ── TransitionResult ─────────────────────────────────────────────────────────

/// The outcome of a [`StateHandler::execute`] call.
///
/// * `errors: None` — execution succeeded; `state_data` holds the input for
///   the next forward state.  Routing is determined by `next_stage` (explicit
///   override) or the handler's [`StateHandler::next`] key (default).
/// * `errors: Some(_)` — execution failed; `state_data` holds the suggested
///   recovery state.  An empty vec signals that the execute fn already routed
///   through [`DomainSolver::diagnose`] (legacy path) and produced a
///   `ProblemState::Diagnosing` in `state_data`.  A non-empty vec causes the
///   orchestrator to call [`StateHandler::diagnose`] for the final routing
///   decision.
pub struct TransitionResult<D: Domain> {
    /// Updated state data: the success output or the suggested recovery state.
    pub state_data: ProblemState<D>,
    /// `None` on success; `Some(errors)` on failure.
    pub errors: Option<Vec<D::Error>>,
    /// Override the next stage key on a successful transition.
    ///
    /// `None` means use the handler's default [`StateHandler::next`] routing.
    /// Set this when the output state does not match what the handler's `next`
    /// field expects — for example, a fan-out that produces
    /// `ProblemState::Interpreting` directly from a `"specifying"` handler
    /// whose `next` is `"solving"`.
    pub next_stage: Option<&'static str>,
    /// Pending fan-out specs to be executed by the orchestrator.
    ///
    /// When set, the orchestrator emits `StateExit` for the current state
    /// *before* launching [`run_fanout`], ensuring the exit event fires
    /// before any sub-spec work begins.  `state_data` is a placeholder and
    /// is ignored when this field is `Some`.
    ///
    /// [`run_fanout`]: super::run_fanout
    pub fan_out: Option<Vec<D::Spec>>,
}

impl<D: Domain> TransitionResult<D> {
    /// Successful transition; routing uses the handler's default `next`.
    pub fn ok(state_data: ProblemState<D>) -> Self {
        Self {
            state_data,
            errors: None,
            next_stage: None,
            fan_out: None,
        }
    }

    /// Successful transition with an explicit next-stage override.
    ///
    /// Use when the output state does not match what the handler's `next`
    /// field expects (e.g. fan-out skipping directly to `"interpreting"`).
    pub fn ok_to(state_data: ProblemState<D>, stage: &'static str) -> Self {
        Self {
            state_data,
            errors: None,
            next_stage: Some(stage),
            fan_out: None,
        }
    }

    /// Route to the `"diagnosing"` arm with a pre-built `Diagnosing` state.
    ///
    /// `state_data` must be `ProblemState::Diagnosing { .. }`.
    pub fn diagnosing(state_data: ProblemState<D>) -> Self {
        debug_assert!(
            matches!(state_data, ProblemState::Diagnosing { .. }),
            "TransitionResult::diagnosing called with non-Diagnosing state"
        );
        Self {
            state_data,
            errors: Some(vec![]),
            next_stage: None,
            fan_out: None,
        }
    }

    /// Validation failure: the handler's `diagnose` callback will be called
    /// with these errors to decide the recovery state.
    pub fn fail(state_data: ProblemState<D>, errors: Vec<D::Error>) -> Self {
        Self {
            state_data,
            errors: Some(errors),
            next_stage: None,
            fan_out: None,
        }
    }

    /// Signal to the orchestrator that specifying produced multiple specs.
    ///
    /// `state_data` is a placeholder (`ProblemState::Specifying(intent)`) that
    /// the orchestrator ignores when `fan_out` is `Some`.  The orchestrator
    /// will emit `StateExit` for the current state and then call
    /// [`run_fanout`] with the provided specs.
    ///
    /// [`run_fanout`]: super::run_fanout
    pub fn pending_fan_out(specs: Vec<D::Spec>, placeholder: ProblemState<D>) -> Self {
        Self {
            state_data: placeholder,
            errors: None,
            next_stage: None,
            fan_out: Some(specs),
        }
    }
}

// ── PipelineResult ────────────────────────────────────────────────────────────

/// Internal exit type for [`Orchestrator::run_pipeline_inner`].
///
/// `Done` is returned when the FSM reaches `ProblemState::Done`.
/// `Stopped` is returned when a `stop_before` stage hint was provided and
/// the FSM reached that stage before executing it.
///
/// [`Orchestrator::run_pipeline_inner`]: super::Orchestrator
pub(super) enum PipelineResult<D: Domain> {
    Done(PipelineOutput<D>),
    /// The FSM was halted just before entering `stop_before`.
    Stopped {
        state: ProblemState<D>,
        #[allow(dead_code)]
        run_ctx: RunContext<D>,
    },
}

impl<D: Domain> PipelineResult<D> {
    /// Unwrap the `Done` variant.  Panics if `Stopped`.
    pub(super) fn done(self) -> PipelineOutput<D> {
        match self {
            Self::Done(o) => o,
            Self::Stopped { .. } => unreachable!("expected PipelineResult::Done, got Stopped"),
        }
    }
}

// ── StateHandler ─────────────────────────────────────────────────────────────

pub(super) type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Table-driven handler for one pipeline state.
///
/// `D` is the domain, `S` is the concrete solver, `Ev` is the domain-event
/// type.
///
/// # `execute`
///
/// Performs the work for this state.  On success it returns a
/// [`TransitionResult`] with `errors: None` and `state_data` set to the input
/// for the next state.  On failure it returns `errors: Some(errors)` and
/// `state_data` set to the suggested recovery state.
///
/// The default handlers (built by [`build_default_handlers`]) route failures
/// through [`DomainSolver::diagnose`] internally: they place the resulting
/// `ProblemState::Diagnosing` in `state_data` and return `errors: Some(vec![])`
/// (empty vec) as a sentinel so [`diagnose`](#structfield.diagnose) passes it
/// through unchanged.
///
/// [`build_default_handlers`]: super::build_default_handlers
///
/// # `diagnose`
///
/// Called when `execute` returns `errors: Some(non_empty)`.  Receives the
/// errors, the number of times this state has been entered (retry count), and
/// the suggested recovery state from `execute`.  Return `Some(state)` to
/// transition there, or `None` to escalate as a fatal error.
///
/// # `next`
///
/// The canonical forward-transition target name, used when `execute` succeeds.
pub struct StateHandler<D: Domain, S: DomainSolver<D>, Ev: DomainEvents = ()> {
    /// Default forward-transition target state name.
    pub next: &'static str,
    /// Async execute function — called with the solver, current state data,
    /// the event stream, a read-only view of accumulated prior-stage outputs,
    /// and the session memory from prior completed turns.
    pub execute: Arc<
        dyn for<'a> Fn(
                &'a mut S,
                ProblemState<D>,
                &'a Option<EventStream<Ev>>,
                &'a RunContext<D>,
                &'a SessionMemory<D>,
            ) -> BoxFuture<'a, TransitionResult<D>>
            + Send
            + Sync,
    >,
    /// Synchronous diagnose function — called on non-empty error results.
    ///
    /// Parameters: `(errors, retry_count, suggested_recovery)`.
    /// Return `Some(next_state)` to transition there, `None` to escalate.
    ///
    /// When this field is `None` (the default), the orchestrator passes the
    /// suggested recovery state through unchanged — equivalent to the closure
    /// `|_, _, r| Some(r)`.  Set a custom closure only when the handler needs
    /// to inspect errors or retry counts to decide whether to escalate.
    pub diagnose: Option<
        Arc<dyn Fn(&[D::Error], u32, ProblemState<D>) -> Option<ProblemState<D>> + Send + Sync>,
    >,
}
