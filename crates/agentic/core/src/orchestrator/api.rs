//! [`Orchestrator`] struct, constructors, and the public async entry points
//! (`run` / `run_pipeline` / `resume` / `retry` / `run_subpipeline`).
//!
//! The actual FSM loop lives in [`super::run_loop`].

use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::sync::Arc;

use crate::domain::Domain;
use crate::events::{DomainEvents, EventStream};
use crate::human_input::{DeferredInputProvider, HumanInputHandle, SuspendedRunData};
use crate::solver::DomainSolver;
use crate::state::ProblemState;

use super::{
    CompletedTurn, OrchestratorError, PipelineOutput, RunContext, SessionMemory, StateHandler,
    build_default_handlers, next_trace_id, transitions::PipelineResult,
};

/// Drives the problem-solving pipeline for a given domain.
///
/// `D` is the [`Domain`] descriptor; `S` is the concrete [`DomainSolver`].
/// `Ev` is the domain-event type for the [`EventStream`]; it defaults to `()`
/// for pipelines that do not need domain-specific events.
///
/// The orchestrator maintains a table of [`StateHandler`]s (one per pipeline
/// state) and dispatches each iteration to the appropriate handler rather than
/// matching on the state enum directly.  This makes adding or overriding
/// individual state handlers straightforward — see [`Orchestrator::with_handlers`].
///
/// Back-edges for the default (wrapping) handlers still pass through
/// `ProblemState::Diagnosing`, which calls [`DomainSolver::diagnose`] to decide
/// where to resume.  Custom handlers (e.g. from `build_analytics_handlers`)
/// can return recovery states directly from their `diagnose` closure, bypassing
/// the legacy Diagnosing arm entirely.
///
/// States listed in the skip set (see [`Orchestrator::with_skip_states`]) are
/// advanced to `handler.next` without calling `execute`.
///
/// # Termination
///
/// The run loop terminates when:
/// - `ProblemState::Done` is reached → `Ok(answer)`,
/// - `DomainSolver::diagnose` returns `Err(fatal)` → `Err(Fatal(fatal))`,
/// - a [`StateHandler::diagnose`] returns `None` → `Err(Fatal(errors[0]))`, or
/// - the iteration counter exceeds `max_iterations` → `Err(MaxIterationsExceeded)`.
pub struct Orchestrator<D: Domain, S: DomainSolver<D>, Ev: DomainEvents = ()> {
    pub(super) solver: S,
    pub(super) handlers: HashMap<&'static str, StateHandler<D, S, Ev>>,
    /// States in this set are skipped: the orchestrator advances to
    /// `handler.next` without calling `execute`.
    pub(super) skip_states: HashSet<&'static str>,
    /// Maximum number of state transitions before the run is aborted.
    pub(super) max_iterations: usize,
    /// Original cap at construction time; used as the increment when the user
    /// asks to continue past the limit.
    pub(super) initial_max_iterations: usize,
    /// Provider for mid-run user prompts (max-iterations pause).
    pub(super) human_input: HumanInputHandle,
    /// Optional event stream for observability.
    pub(super) event_tx: Option<EventStream<Ev>>,
    /// Accumulated history from prior completed runs in this session.
    pub(super) memory: SessionMemory<D>,
    pub(super) _phantom: PhantomData<D>,
}

impl<D: Domain, S: DomainSolver<D> + 'static, Ev: DomainEvents> Orchestrator<D, S, Ev> {
    /// Create an orchestrator with default handlers and a cap of 1 000 iterations.
    pub fn new(solver: S) -> Self {
        Self::with_max_iterations(solver, 1_000)
    }

    /// Create an orchestrator with an explicit iteration cap.
    pub fn with_max_iterations(solver: S, max_iterations: usize) -> Self {
        let mut skip_states = HashSet::new();
        // Seed the skip set from the solver type's static declaration.
        skip_states.extend(<S as DomainSolver<D>>::SKIP_STATES.iter().copied());
        Self {
            handlers: build_default_handlers::<D, S, Ev>(),
            solver,
            skip_states,
            max_iterations,
            initial_max_iterations: max_iterations,
            human_input: Arc::new(DeferredInputProvider),
            event_tx: None,
            memory: SessionMemory::default(),
            _phantom: PhantomData,
        }
    }

    /// Replace the entire handler table with a custom one.
    ///
    /// Use `build_analytics_handlers` (or a domain-specific equivalent) to
    /// obtain a pre-built table, then pass it here.
    pub fn with_handlers(
        mut self,
        handlers: HashMap<&'static str, StateHandler<D, S, Ev>>,
    ) -> Self {
        self.handlers = handlers;
        self
    }

    /// Mark one or more states as skipped.
    ///
    /// Skipped states are advanced to `handler.next` without calling `execute`.
    /// The current `ProblemState` data is passed as-is; the handler must still
    /// be present in the table so that `next` can be resolved.
    pub fn with_skip_states(mut self, states: &[&'static str]) -> Self {
        self.skip_states.extend(states.iter().copied());
        self
    }

    /// Attach a human-input provider for mid-run prompts (e.g. max-iterations pause).
    ///
    /// The CLI wires `StdinInputProvider` here so the orchestrator can ask the
    /// user whether to continue when the iteration cap is hit.  The default
    /// ([`DeferredInputProvider`]) causes an immediate hard stop, preserving the
    /// existing behavior for API/server deployments.
    pub fn with_human_input(mut self, provider: HumanInputHandle) -> Self {
        self.human_input = provider;
        self
    }

    /// Attach an event stream so the orchestrator emits observability events.
    pub fn with_events(mut self, tx: EventStream<Ev>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    /// Set the maximum number of prior turns retained for multi-turn context.
    /// Defaults to 10.
    pub fn with_max_memory_turns(mut self, n: usize) -> Self {
        self.memory = SessionMemory::new(n);
        self
    }

    /// Read-only access to session memory.
    pub fn memory(&self) -> &SessionMemory<D> {
        &self.memory
    }

    /// Clear session memory (e.g. when the user starts a new conversation topic).
    pub fn clear_memory(&mut self) {
        self.memory.clear();
    }

    /// Run the pipeline to completion starting from `initial_intent`.
    ///
    /// A fresh `trace_id` is generated for every call.  On success the
    /// completed turn is recorded in session memory.
    ///
    /// # Flow
    ///
    /// The full FSM pipeline runs from Clarifying through Done.  Any
    /// multi-spec fan-out is handled inside the Specifying stage handler
    /// (see [`build_default_handlers`]) — `run` itself is a single,
    /// linear FSM invocation.
    pub async fn run(
        &mut self,
        initial_intent: D::Intent,
    ) -> Result<D::Answer, OrchestratorError<D>>
    where
        D::Intent: Clone,
        D::Spec: Clone,
        D::Answer: Clone,
    {
        let parent_trace = next_trace_id();
        let run_ctx: RunContext<D> = RunContext {
            intent: None,
            spec: None,
            retry_ctx: None,
        };
        let output = self
            .run_pipeline_inner(
                ProblemState::Clarifying(initial_intent),
                &parent_trace,
                run_ctx,
                None,
            )
            .await?
            .done();
        self.memory.push(CompletedTurn {
            intent: output.intent,
            spec: output.spec,
            answer: output.answer.clone(),
            trace_id: parent_trace,
        });
        Ok(output.answer)
    }

    /// Execute the FSM pipeline to completion without touching session memory.
    ///
    /// Starts from `Clarifying(initial_intent)` — the full pipeline including
    /// clarify, specify, solve, execute, and interpret.
    ///
    /// Returns a [`PipelineOutput`] containing the answer and accumulated
    /// context.  The caller (`run` or a future scatter-gather wrapper) is
    /// responsible for deciding what to record in memory.
    pub async fn run_pipeline(
        &mut self,
        initial_intent: D::Intent,
        trace_id: &str,
    ) -> Result<PipelineOutput<D>, OrchestratorError<D>>
    where
        D::Intent: Clone,
        D::Spec: Clone,
        D::Answer: Clone,
    {
        let run_ctx: RunContext<D> = RunContext {
            intent: None,
            spec: None,
            retry_ctx: None,
        };
        Ok(self
            .run_pipeline_inner(
                ProblemState::Clarifying(initial_intent),
                trace_id,
                run_ctx,
                None,
            )
            .await?
            .done())
    }

    /// Get a mutable reference to the solver.
    ///
    /// Useful for replacing the catalog before resuming from a delegation
    /// (e.g., after the builder has modified semantic layer files).
    pub fn solver_mut(&mut self) -> &mut S {
        &mut self.solver
    }

    /// Resume a suspended pipeline with the user's answer to an `ask_user` call.
    ///
    /// Reconstructs the correct starting [`ProblemState`] from `data` via
    /// [`DomainSolver::problem_state_from_resume`], injects the answer into
    /// the solver via [`DomainSolver::set_resume_data`], then re-runs the
    /// pipeline from the suspended state.
    ///
    /// On success the completed turn is recorded in session memory (same as
    /// [`run`]).
    ///
    /// [`run`]: Orchestrator::run
    pub async fn resume(
        &mut self,
        data: SuspendedRunData,
        answer: String,
    ) -> Result<D::Answer, OrchestratorError<D>>
    where
        D::Intent: Clone,
        D::Spec: Clone,
        D::Answer: Clone,
    {
        use crate::human_input::ResumeInput;
        let trace_id = data.trace_id.clone();
        // Set resume data BEFORE problem_state_from_resume so the solver
        // can access the answer when building the start state (e.g. to
        // parse delegation output into a real AnalyticsResult).
        self.solver.set_resume_data(ResumeInput {
            data: data.clone(),
            answer,
        });
        let start_state = match self.solver.problem_state_from_resume(&data, &self.memory) {
            Some(s) => s,
            None => return Err(OrchestratorError::ResumeNotSupported),
        };
        let mut run_ctx: RunContext<D> = RunContext {
            intent: None,
            spec: None,
            retry_ctx: None,
        };
        self.solver.populate_resume_context(&data, &mut run_ctx);
        let output = self
            .run_pipeline_inner(start_state, &trace_id, run_ctx, None)
            .await?
            .done();
        self.memory.push(CompletedTurn {
            intent: output.intent,
            spec: output.spec,
            answer: output.answer.clone(),
            trace_id,
        });
        Ok(output.answer)
    }

    /// Retry a failed run from a previously stored checkpoint.
    ///
    /// Like [`resume`] but without injecting a user answer.  The checkpoint
    /// data (a [`SuspendedRunData`]) is the same struct used for HITL
    /// suspension — the solver's [`problem_state_from_resume`] reconstructs
    /// the `ProblemState` to re-enter.
    ///
    /// The caller is responsible for setting up any pre-computed state on
    /// the solver before calling this (e.g. `pre_computed_specs` or
    /// `pre_solved_sqls` for fan-out retry).
    ///
    /// [`resume`]: Orchestrator::resume
    /// [`problem_state_from_resume`]: crate::solver::DomainSolver::problem_state_from_resume
    pub async fn retry(&mut self, data: SuspendedRunData) -> Result<D::Answer, OrchestratorError<D>>
    where
        D::Intent: Clone,
        D::Spec: Clone,
        D::Answer: Clone,
    {
        let trace_id = data.trace_id.clone();
        let start_state = match self.solver.problem_state_from_resume(&data, &self.memory) {
            Some(s) => s,
            None => return Err(OrchestratorError::ResumeNotSupported),
        };
        // No set_resume_data — retry does not inject a user answer.
        let run_ctx: RunContext<D> = RunContext {
            intent: None,
            spec: None,
            retry_ctx: None,
        };
        let output = self
            .run_pipeline_inner(start_state, &trace_id, run_ctx, None)
            .await?
            .done();
        self.memory.push(CompletedTurn {
            intent: output.intent,
            spec: output.spec,
            answer: output.answer.clone(),
            trace_id,
        });
        Ok(output.answer)
    }

    /// Retrieve checkpoint data stored by the orchestrator on fatal error.
    ///
    /// After [`run`] or [`resume`] returns [`OrchestratorError::Fatal`],
    /// call this to extract the checkpoint data that enables retry.
    /// Returns `None` if the solver's [`build_checkpoint`] returned `None`
    /// or if the run did not end with a fatal error.
    ///
    /// [`run`]: Orchestrator::run
    /// [`resume`]: Orchestrator::resume
    /// [`build_checkpoint`]: crate::solver::DomainSolver::build_checkpoint
    pub fn take_checkpoint(&mut self) -> Option<SuspendedRunData> {
        self.solver.take_suspension_data()
    }

    /// Execute a partial pipeline from `start`, stopping just before
    /// `stop_before` is executed.
    ///
    /// Returns the [`ProblemState`] that was about to enter `stop_before`.
    /// The caller can inspect it and continue the pipeline independently
    /// (e.g. to run fan-out sub-specs in parallel).
    ///
    /// # Example — run Solving+Executing, stop before Interpreting
    ///
    /// ```ignore
    /// let state = orchestrator
    ///     .run_subpipeline(ProblemState::Solving(spec), &trace_id, "interpreting")
    ///     .await?;
    /// ```
    pub async fn run_subpipeline(
        &mut self,
        start: ProblemState<D>,
        trace_id: &str,
        stop_before: &'static str,
    ) -> Result<ProblemState<D>, OrchestratorError<D>>
    where
        D::Intent: Clone,
        D::Spec: Clone,
        D::Answer: Clone,
    {
        let run_ctx = RunContext {
            intent: None,
            spec: None,
            retry_ctx: None,
        };
        match self
            .run_pipeline_inner(start, trace_id, run_ctx, Some(stop_before))
            .await?
        {
            PipelineResult::Stopped { state, .. } => Ok(state),
            PipelineResult::Done(_) => panic!(
                "run_subpipeline: pipeline reached Done without hitting stop_before = '{stop_before}'"
            ),
        }
    }

    /// Borrow the underlying solver (e.g. to inspect accumulated state after
    /// a run).
    pub fn solver(&self) -> &S {
        &self.solver
    }

    /// Consume the orchestrator and return the solver.
    pub fn into_solver(self) -> S {
        self.solver
    }
}
