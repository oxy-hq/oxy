use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{
    back_target::{BackTarget, RetryContext},
    domain::Domain,
    events::{CoreEvent, DomainEvents, Event, EventStream, HumanInputQuestion, Outcome},
    human_input::{DeferredInputProvider, HumanInputHandle, SuspendedRunData},
    solver::{DomainSolver, FanoutWorker},
    state::ProblemState,
};

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
    pub retry_ctx: Option<RetryContext>,
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

fn state_name<D: Domain>(state: &ProblemState<D>) -> &'static str {
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
fn stage_order(name: &str) -> u8 {
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

    /// The LLM called `ask_user` with a [`DeferredInputProvider`] — the run
    /// is paused until the user provides an answer.
    ///
    /// The caller should:
    /// 1. Persist `resume_data` (e.g. to a database).
    /// 2. Present `prompt` and `suggestions` to the user.
    /// 3. On the user's next message, call [`Orchestrator::resume`] with the
    ///    stored data and the user's answer.
    ///
    /// [`DeferredInputProvider`]: crate::human_input::DeferredInputProvider
    /// [`Orchestrator::resume`]: crate::orchestrator::Orchestrator::resume
    Suspended {
        /// Questions posed to the user (one or more).
        questions: Vec<HumanInputQuestion>,
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
async fn emit<Ev: DomainEvents>(tx: &Option<EventStream<Ev>>, event: CoreEvent) {
    if let Some(tx) = tx {
        let _ = tx.send(Event::Core(event)).await;
    }
}

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
    /// [`run_fanout`]: run_fanout
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
    /// [`run_fanout`]: run_fanout
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
enum PipelineResult<D: Domain> {
    Done(PipelineOutput<D>),
    /// The FSM was halted just before entering `stop_before`.
    Stopped {
        state: ProblemState<D>,
        run_ctx: RunContext<D>,
    },
}

impl<D: Domain> PipelineResult<D> {
    /// Unwrap the `Done` variant.  Panics if `Stopped`.
    fn done(self) -> PipelineOutput<D> {
        match self {
            Self::Done(o) => o,
            Self::Stopped { .. } => unreachable!("expected PipelineResult::Done, got Stopped"),
        }
    }
}

// ── StateHandler ─────────────────────────────────────────────────────────────

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

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

// ── Fan-out helper ────────────────────────────────────────────────────────────

/// Execute solve+execute for each spec in `specs`, merge the results, and
/// return a [`TransitionResult`] pointing at the Interpreting state.
///
/// Emits [`CoreEvent::FanOut`], [`CoreEvent::SubSpecStart`], and
/// [`CoreEvent::SubSpecEnd`] events for each sub-spec.  On any sub-spec
/// failure or merge failure, returns a `TransitionResult` that routes through
/// [`ProblemState::Diagnosing`].
///
/// # Panics
///
/// Panics (via `.expect`) if `ctx_intent` is `None`, which must not happen
/// when Specifying is reached normally (the orchestrator sets `ctx.intent`
/// after Clarifying).
pub async fn run_fanout<D, S, Ev>(
    solver: &mut S,
    specs: Vec<D::Spec>,
    ctx: &RunContext<D>,
    mem: &SessionMemory<D>,
    ctx_intent: Option<D::Intent>,
    events: &Option<EventStream<Ev>>,
) -> TransitionResult<D>
where
    D: Domain,
    S: DomainSolver<D>,
    Ev: DomainEvents,
{
    let total = specs.len();
    let fan_trace = next_trace_id();
    emit(
        events,
        CoreEvent::FanOut {
            spec_count: total,
            trace_id: fan_trace.clone(),
        },
    )
    .await;

    // ── Try concurrent path ──────────────────────────────────────────────
    if let Some(worker) = solver.fanout_worker::<Ev>() {
        // Build owned copies for the concurrent tasks.
        let ctx_owned = RunContext {
            intent: ctx.intent.clone(),
            spec: ctx.spec.clone(),
            retry_ctx: ctx.retry_ctx.clone(),
        };
        let mem_owned = mem.clone_shallow();
        let max_retries = solver.max_fanout_retries();
        let results = run_fanout_concurrent::<D, Ev>(
            worker,
            specs,
            total,
            fan_trace.clone(),
            Arc::new(ctx_owned),
            Arc::new(mem_owned),
            events.clone(),
            max_retries,
        )
        .await;

        return collect_fanout_results::<D, S>(solver, results, ctx_intent);
    }

    // ── Serial fallback ──────────────────────────────────────────────────
    let mut results = Vec::with_capacity(total);
    for (index, spec) in specs.into_iter().enumerate() {
        let sub_trace = child_trace_id(&fan_trace, index);
        emit(
            events,
            CoreEvent::SubSpecStart {
                index,
                total,
                trace_id: sub_trace.clone(),
            },
        )
        .await;

        emit(
            events,
            CoreEvent::StateEnter {
                state: "solving".into(),
                revision: 0,
                trace_id: sub_trace.clone(),
                sub_spec_index: None,
            },
        )
        .await;
        let solution = match solver.solve(spec, ctx, mem).await {
            Ok(s) => s,
            Err((err, back)) => {
                emit(
                    events,
                    CoreEvent::StateExit {
                        state: "solving".into(),
                        outcome: Outcome::Failed,
                        trace_id: sub_trace,
                        sub_spec_index: None,
                    },
                )
                .await;
                return TransitionResult::diagnosing(ProblemState::Diagnosing { error: err, back });
            }
        };
        emit(
            events,
            CoreEvent::StateExit {
                state: "solving".into(),
                outcome: Outcome::Advanced,
                trace_id: sub_trace.clone(),
                sub_spec_index: None,
            },
        )
        .await;

        emit(
            events,
            CoreEvent::StateEnter {
                state: "executing".into(),
                revision: 0,
                trace_id: sub_trace.clone(),
                sub_spec_index: None,
            },
        )
        .await;
        let result = match solver.execute(solution, ctx, mem).await {
            Ok(r) => r,
            Err((err, back)) => {
                emit(
                    events,
                    CoreEvent::StateExit {
                        state: "executing".into(),
                        outcome: Outcome::Failed,
                        trace_id: sub_trace,
                        sub_spec_index: None,
                    },
                )
                .await;
                return TransitionResult::diagnosing(ProblemState::Diagnosing { error: err, back });
            }
        };
        emit(
            events,
            CoreEvent::StateExit {
                state: "executing".into(),
                outcome: Outcome::Advanced,
                trace_id: sub_trace.clone(),
                sub_spec_index: None,
            },
        )
        .await;

        emit(
            events,
            CoreEvent::SubSpecEnd {
                index,
                trace_id: sub_trace,
            },
        )
        .await;
        results.push(result);
    }

    match solver.merge_results(results) {
        Ok(merged) => TransitionResult::ok_to(ProblemState::Interpreting(merged), "interpreting"),
        Err(err) => {
            let intent_for_back = ctx_intent.expect("intent must be set before specifying");
            TransitionResult::diagnosing(ProblemState::Diagnosing {
                error: err,
                back: BackTarget::Specify(intent_for_back, Default::default()),
            })
        }
    }
}

// ── Concurrent fan-out helpers ───────────────────────────────────────────────

/// Spawn concurrent tasks for each spec and collect results.
///
/// Each sub-spec is retried up to `max_retries` times on failure.  The error
/// message from the failed attempt is injected into [`RetryContext`] so the
/// LLM can correct its SQL on the next attempt.  Only the failed sub-spec is
/// retried; successful ones are kept.
async fn run_fanout_concurrent<D, Ev>(
    worker: Arc<dyn FanoutWorker<D, Ev>>,
    specs: Vec<D::Spec>,
    total: usize,
    fan_trace: String,
    ctx: Arc<RunContext<D>>,
    mem: Arc<SessionMemory<D>>,
    events: Option<EventStream<Ev>>,
    max_retries: u32,
) -> Vec<(usize, Result<D::Result, (D::Error, BackTarget<D>)>)>
where
    D: Domain,
    Ev: DomainEvents,
{
    let mut handles = Vec::with_capacity(total);
    for (index, spec) in specs.into_iter().enumerate() {
        let w = Arc::clone(&worker);
        let ev = events.clone();
        let sub_trace = child_trace_id(&fan_trace, index);
        let ctx = Arc::clone(&ctx);
        let mem = Arc::clone(&mem);

        let handle = tokio::spawn(async move {
            emit(
                &ev,
                CoreEvent::SubSpecStart {
                    index,
                    total,
                    trace_id: sub_trace.clone(),
                },
            )
            .await;

            // Retry loop: on failure, re-attempt solve+execute with error context.
            let mut retry_ctx: Option<RetryContext> = ctx.retry_ctx.clone();

            for attempt in 0..=max_retries {
                let attempt_run_ctx = RunContext {
                    intent: ctx.intent.clone(),
                    spec: ctx.spec.clone(),
                    retry_ctx: retry_ctx.clone(),
                };

                let result = w
                    .solve_and_execute(spec.clone(), index, total, &ev, &attempt_run_ctx, &mem)
                    .await;

                match result {
                    Ok(r) => {
                        emit(
                            &ev,
                            CoreEvent::SubSpecEnd {
                                index,
                                trace_id: sub_trace,
                            },
                        )
                        .await;
                        return (index, Ok(r));
                    }
                    Err((err, back)) => {
                        if attempt >= max_retries {
                            emit(
                                &ev,
                                CoreEvent::SubSpecEnd {
                                    index,
                                    trace_id: sub_trace,
                                },
                            )
                            .await;
                            return (index, Err((err, back)));
                        }

                        let err_msg = err.to_string();
                        emit(
                            &ev,
                            CoreEvent::BackEdge {
                                from: "executing".into(),
                                to: "solving".into(),
                                reason: err_msg.clone(),
                                trace_id: sub_trace.clone(),
                            },
                        )
                        .await;

                        retry_ctx = Some(match retry_ctx.take() {
                            Some(existing) => existing.advance(err_msg),
                            None => RetryContext {
                                errors: vec![err_msg],
                                attempt: 1,
                                previous_output: None,
                            },
                        });
                    }
                }
            }

            unreachable!("retry loop must exit via return")
        });
        handles.push(handle);
    }

    let mut outcomes = Vec::with_capacity(total);
    for handle in handles {
        match handle.await {
            Ok(result) => outcomes.push(result),
            Err(join_err) => {
                // Task panicked — shouldn't happen, but handle gracefully.
                eprintln!("fanout task panicked: {join_err}");
            }
        }
    }
    outcomes
}

/// Collect concurrent fan-out results: merge successes or route to diagnosing.
fn collect_fanout_results<D, S>(
    solver: &S,
    outcomes: Vec<(usize, Result<D::Result, (D::Error, BackTarget<D>)>)>,
    ctx_intent: Option<D::Intent>,
) -> TransitionResult<D>
where
    D: Domain,
    S: DomainSolver<D>,
{
    let mut successes = Vec::new();
    let mut first_error: Option<(D::Error, BackTarget<D>)> = None;

    for (_index, result) in outcomes {
        match result {
            Ok(r) => successes.push(r),
            Err((err, back)) => {
                if first_error.is_none() {
                    first_error = Some((err, back));
                }
                // Continue collecting — we let all tasks finish.
            }
        }
    }

    if let Some((err, back)) = first_error {
        // At least one sub-spec failed. Route to diagnosing with the first error.
        return TransitionResult::diagnosing(ProblemState::Diagnosing { error: err, back });
    }

    match solver.merge_results(successes) {
        Ok(merged) => TransitionResult::ok_to(ProblemState::Interpreting(merged), "interpreting"),
        Err(err) => {
            let intent_for_back = ctx_intent.expect("intent must be set before specifying");
            TransitionResult::diagnosing(ProblemState::Diagnosing {
                error: err,
                back: BackTarget::Specify(intent_for_back, Default::default()),
            })
        }
    }
}

// ── Default handler builders ──────────────────────────────────────────────────

/// Build the default set of state handlers that delegate to the corresponding
/// [`DomainSolver`] methods.
///
/// On failure each handler wraps the `(error, BackTarget)` pair in
/// `ProblemState::Diagnosing` and passes it through the legacy Diagnosing arm
/// in the orchestrator loop, which in turn calls `DomainSolver::diagnose`.
pub fn build_default_handlers<D, S, Ev>() -> HashMap<&'static str, StateHandler<D, S, Ev>>
where
    D: Domain + 'static,
    S: DomainSolver<D> + 'static,
    Ev: DomainEvents,
{
    let mut map: HashMap<&'static str, StateHandler<D, S, Ev>> = HashMap::new();

    // ── clarifying ────────────────────────────────────────────────────────────
    map.insert(
        "clarifying",
        StateHandler {
            next: "specifying",
            execute: Arc::new(|solver, state, _events, run_ctx, memory| {
                Box::pin(async move {
                    let data = match state {
                        ProblemState::Clarifying(d) => d,
                        _ => unreachable!("clarifying handler called with wrong state"),
                    };
                    match solver.clarify(data, run_ctx, memory).await {
                        Ok(output) => TransitionResult::ok(ProblemState::Specifying(output)),
                        Err((err, back)) => {
                            TransitionResult::diagnosing(ProblemState::Diagnosing {
                                error: err,
                                back,
                            })
                        }
                    }
                })
            }),
            diagnose: None,
        },
    );

    // ── specifying ────────────────────────────────────────────────────────────
    map.insert(
        "specifying",
        StateHandler {
            next: "solving",
            execute: Arc::new(|solver, state, _events, run_ctx, memory| {
                // Extract intent once so it can be used in multiple error branches.
                let ctx_intent = run_ctx.intent.clone();
                Box::pin(async move {
                    let data = match state {
                        ProblemState::Specifying(d) => d,
                        _ => unreachable!("specifying handler called with wrong state"),
                    };
                    match solver.specify(data, run_ctx, memory).await {
                        Ok(specs) if specs.len() == 1 => {
                            // Fast path: single spec → standard Solving transition.
                            TransitionResult::ok(ProblemState::Solving(
                                specs.into_iter().next().unwrap(),
                            ))
                        }
                        Ok(specs) if specs.is_empty() => {
                            // Empty specs is an error — specify must return at least one.
                            // Use the empty-vec sentinel so the orchestrator retries.
                            let intent_for_back = ctx_intent
                                .clone()
                                .expect("intent must be set before specifying");
                            TransitionResult {
                                state_data: ProblemState::Specifying(intent_for_back),
                                errors: Some(vec![]),
                                next_stage: None,
                                fan_out: None,
                            }
                        }
                        Ok(specs) => {
                            // Fan-out: yield control back to orchestrator so that
                            // StateExit for "specifying" fires before any sub-spec
                            // work begins.  The orchestrator calls run_fanout after
                            // emitting the exit event.
                            let intent_placeholder =
                                ctx_intent.expect("intent must be set before specifying");
                            TransitionResult::pending_fan_out(
                                specs,
                                ProblemState::Specifying(intent_placeholder),
                            )
                        }
                        Err((err, back)) => {
                            TransitionResult::diagnosing(ProblemState::Diagnosing {
                                error: err,
                                back,
                            })
                        }
                    }
                })
            }),
            diagnose: None,
        },
    );

    // ── solving ───────────────────────────────────────────────────────────────
    map.insert(
        "solving",
        StateHandler {
            next: "executing",
            execute: Arc::new(|solver, state, _events, run_ctx, memory| {
                Box::pin(async move {
                    let data = match state {
                        ProblemState::Solving(d) => d,
                        _ => unreachable!("solving handler called with wrong state"),
                    };
                    match solver.solve(data, run_ctx, memory).await {
                        Ok(output) => TransitionResult::ok(ProblemState::Executing(output)),
                        Err((err, back)) => {
                            TransitionResult::diagnosing(ProblemState::Diagnosing {
                                error: err,
                                back,
                            })
                        }
                    }
                })
            }),
            diagnose: None,
        },
    );

    // ── executing ─────────────────────────────────────────────────────────────
    map.insert(
        "executing",
        StateHandler {
            next: "interpreting",
            execute: Arc::new(|solver, state, _events, run_ctx, memory| {
                Box::pin(async move {
                    let data = match state {
                        ProblemState::Executing(d) => d,
                        _ => unreachable!("executing handler called with wrong state"),
                    };
                    match solver.execute(data, run_ctx, memory).await {
                        Ok(output) => TransitionResult::ok(ProblemState::Interpreting(output)),
                        Err((err, back)) => {
                            TransitionResult::diagnosing(ProblemState::Diagnosing {
                                error: err,
                                back,
                            })
                        }
                    }
                })
            }),
            diagnose: None,
        },
    );

    // ── interpreting ──────────────────────────────────────────────────────────
    map.insert(
        "interpreting",
        StateHandler {
            next: "done",
            execute: Arc::new(|solver, state, _events, run_ctx, memory| {
                Box::pin(async move {
                    let data = match state {
                        ProblemState::Interpreting(d) => d,
                        _ => unreachable!("interpreting handler called with wrong state"),
                    };
                    match solver.interpret(data, run_ctx, memory).await {
                        Ok(output) => TransitionResult::ok(ProblemState::Done(output)),
                        Err((err, back)) => {
                            TransitionResult::diagnosing(ProblemState::Diagnosing {
                                error: err,
                                back,
                            })
                        }
                    }
                })
            }),
            diagnose: None,
        },
    );

    map
}

// ── Orchestrator ──────────────────────────────────────────────────────────────

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
/// where to resume.  Custom handlers (e.g. from [`build_analytics_handlers`])
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
    solver: S,
    handlers: HashMap<&'static str, StateHandler<D, S, Ev>>,
    /// States in this set are skipped: the orchestrator advances to
    /// `handler.next` without calling `execute`.
    skip_states: HashSet<&'static str>,
    /// Maximum number of state transitions before the run is aborted.
    max_iterations: usize,
    /// Original cap at construction time; used as the increment when the user
    /// asks to continue past the limit.
    initial_max_iterations: usize,
    /// Provider for mid-run user prompts (max-iterations pause).
    human_input: HumanInputHandle,
    /// Optional event stream for observability.
    event_tx: Option<EventStream<Ev>>,
    /// Accumulated history from prior completed runs in this session.
    memory: SessionMemory<D>,
    _phantom: PhantomData<D>,
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
    /// Use [`build_analytics_handlers`] (or a domain-specific equivalent) to
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
        let start_state = match self.solver.problem_state_from_resume(&data, &self.memory) {
            Some(s) => s,
            None => return Err(OrchestratorError::ResumeNotSupported),
        };
        self.solver.set_resume_data(ResumeInput { data, answer });
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

    /// Core FSM loop that drives the pipeline from an arbitrary initial state.
    ///
    /// Callers provide the `initial_state` (e.g. `Clarifying` for a full run,
    /// `Specifying` when clarify has already completed) and a pre-populated
    /// [`RunContext`].  The FSM loop iterates until `Done`, a fatal error, or
    /// `max_iterations` is exceeded.
    ///
    /// When `stop_before` is `Some(stage)`, the loop halts and returns
    /// [`PipelineResult::Stopped`] just before executing that stage.  Used by
    /// [`Orchestrator::run_subpipeline`] to run a partial pipeline.
    async fn run_pipeline_inner(
        &mut self,
        initial_state: ProblemState<D>,
        trace_id: &str,
        run_ctx: RunContext<D>,
        stop_before: Option<&'static str>,
    ) -> Result<PipelineResult<D>, OrchestratorError<D>>
    where
        D::Intent: Clone,
        D::Spec: Clone,
        D::Answer: Clone,
    {
        let mut run_ctx = run_ctx;
        let mut state = initial_state;
        // `current_stage` is the handler key to dispatch next iteration.
        // Maintained separately from `state` so handlers can route explicitly
        // via `TransitionResult::next_stage` without relying on state_name().
        let mut current_stage: &'static str = state_name(&state);
        let mut iterations: usize = 0;
        let mut revisions: HashMap<&'static str, u32> = HashMap::new();
        // Tracks the last active worker stage for Diagnosing back-edge events.
        let mut last_worker_stage: &'static str = current_stage;

        loop {
            if iterations >= self.max_iterations {
                let prompt = "Max iterations reached. Would you like to continue?";
                let suggestions = vec!["continue".to_string(), "stop".to_string()];
                match self.human_input.request_sync(prompt, &suggestions) {
                    Ok(answer)
                        if answer.trim().eq_ignore_ascii_case("continue")
                            || answer.trim() == "1" =>
                    {
                        self.max_iterations += self.initial_max_iterations;
                    }
                    _ => {
                        emit(
                            &self.event_tx,
                            CoreEvent::Error {
                                message: "max iterations exceeded".to_string(),
                                trace_id: trace_id.to_string(),
                            },
                        )
                        .await;
                        return Err(OrchestratorError::MaxIterationsExceeded);
                    }
                }
            }
            iterations += 1;

            match current_stage {
                // ── Terminal ──────────────────────────────────────────────────
                "done" => {
                    let answer = match state {
                        ProblemState::Done(a) => a,
                        _ => unreachable!("'done' current_stage with non-Done state data"),
                    };
                    emit(
                        &self.event_tx,
                        CoreEvent::Done {
                            trace_id: trace_id.to_string(),
                        },
                    )
                    .await;
                    return Ok(PipelineResult::Done(PipelineOutput {
                        answer,
                        intent: run_ctx
                            .intent
                            .expect("intent must be set by Clarifying before Done"),
                        spec: run_ctx.spec,
                    }));
                }

                // ── Legacy Diagnosing arm ─────────────────────────────────────
                // Default handlers produce ProblemState::Diagnosing on failure
                // so that DomainSolver::diagnose is called here, preserving
                // backward compatibility with existing DomainSolver impls.
                "diagnosing" => {
                    let (error, back) = match state {
                        ProblemState::Diagnosing { error, back } => (error, back),
                        _ => unreachable!(
                            "'diagnosing' current_stage with non-Diagnosing state data"
                        ),
                    };
                    let from = last_worker_stage;

                    // ── ask_user suspension short-circuit ─────────────────────
                    // Must happen BEFORE retry_ctx() is called (Suspend has no
                    // meaningful RetryContext and the unreachable! would fire).
                    if let BackTarget::Suspend { ref questions } = back {
                        let mut data = self.solver.take_suspension_data()
                            .expect("solver must call store_suspension_data before returning BackTarget::Suspend");
                        data.trace_id = trace_id.to_string();
                        emit(
                            &self.event_tx,
                            CoreEvent::AwaitingHumanInput {
                                questions: questions.clone(),
                                from_state: from.to_string(),
                                trace_id: trace_id.to_string(),
                            },
                        )
                        .await;
                        emit(
                            &self.event_tx,
                            CoreEvent::StateExit {
                                state: from.to_string(),
                                outcome: Outcome::Suspended,
                                trace_id: trace_id.to_string(),
                                sub_spec_index: None,
                            },
                        )
                        .await;
                        return Err(OrchestratorError::Suspended {
                            questions: questions.clone(),
                            resume_data: data,
                            trace_id: trace_id.to_string(),
                        });
                    }

                    let retry_ctx = back.retry_ctx().clone();
                    let back_edge_reason: String = match retry_ctx.errors.last() {
                        Some(e) => e.clone(),
                        None => format!("{error}"),
                    };
                    match self.solver.diagnose(error, back, &run_ctx).await {
                        Ok(recovered) => {
                            run_ctx.retry_ctx = Some(retry_ctx);
                            let to = state_name(&recovered);
                            let outcome = if to == from {
                                Outcome::Retry
                            } else if stage_order(to) > stage_order(from) {
                                // The recovered state is *ahead* of the failing state
                                // (e.g. executing → interpreting via ValueAnomaly).
                                // Treat this as a forward advance, not a back-edge.
                                Outcome::Advanced
                            } else {
                                Outcome::BackTracked
                            };
                            emit(
                                &self.event_tx,
                                CoreEvent::StateExit {
                                    state: from.into(),
                                    outcome: outcome.clone(),
                                    trace_id: trace_id.to_string(),
                                    sub_spec_index: None,
                                },
                            )
                            .await;
                            // Only emit BackEdge for genuine backward or retry
                            // transitions — forward advances are not back-edges.
                            if outcome != Outcome::Advanced {
                                emit(
                                    &self.event_tx,
                                    CoreEvent::BackEdge {
                                        from: from.into(),
                                        to: to.into(),
                                        reason: back_edge_reason,
                                        trace_id: trace_id.to_string(),
                                    },
                                )
                                .await;
                            }
                            current_stage = to;
                            state = recovered;
                        }
                        Err(fatal) => {
                            emit(
                                &self.event_tx,
                                CoreEvent::StateExit {
                                    state: from.into(),
                                    outcome: Outcome::Failed,
                                    trace_id: trace_id.to_string(),
                                    sub_spec_index: None,
                                },
                            )
                            .await;
                            emit(
                                &self.event_tx,
                                CoreEvent::Error {
                                    message: "fatal error from diagnose".to_string(),
                                    trace_id: trace_id.to_string(),
                                },
                            )
                            .await;
                            // Store checkpoint for retry.
                            if let Some(cp) = self.solver.build_checkpoint(from, &run_ctx, None) {
                                self.solver.store_suspension_data(cp);
                            }
                            return Err(OrchestratorError::Fatal(fatal));
                        }
                    }
                }

                // ── Table-driven worker states ─────────────────────────────────
                sname => {
                    // ── Sub-pipeline stop ─────────────────────────────────────
                    // Halt before executing this stage if the caller requested it.
                    if stop_before == Some(sname) {
                        return Ok(PipelineResult::Stopped { state, run_ctx });
                    }

                    // Update RunContext from state data before execute / should_skip.
                    match &state {
                        ProblemState::Clarifying(intent) => {
                            // Capture the raw intent as a fallback for the GeneralInquiry
                            // shortcut, which jumps directly from clarifying → done without
                            // ever entering the specifying stage.
                            run_ctx.intent = Some(intent.clone());
                        }
                        ProblemState::Specifying(intent) => {
                            run_ctx.intent = Some(intent.clone());
                        }
                        ProblemState::Solving(spec) => {
                            run_ctx.spec = Some(spec.clone());
                        }
                        _ => {}
                    }

                    // ── Skip check (static SKIP_STATES / dynamic should_skip) ──
                    // The solver may bypass this state entirely.  No events are
                    // emitted and execute is not called when a skip occurs.
                    if let Some(next_state) = self.solver.should_skip(sname, &state, &run_ctx) {
                        current_stage = state_name(&next_state);
                        state = next_state;
                        continue;
                    }

                    last_worker_stage = sname;

                    // Clone the Arcs so we can release the borrow on
                    // self.handlers before taking &mut self.solver.
                    let (execute_fn, diagnose_fn, handler_next) = {
                        let h = self
                            .handlers
                            .get(sname)
                            .unwrap_or_else(|| panic!("no handler for state '{sname}'"));
                        (Arc::clone(&h.execute), h.diagnose.clone(), h.next)
                    };

                    let _rev = Self::enter(&self.event_tx, sname, &mut revisions, trace_id).await;

                    let result = execute_fn(
                        &mut self.solver,
                        state,
                        &self.event_tx,
                        &run_ctx,
                        &self.memory,
                    )
                    .await;

                    match result.errors {
                        // ── Success ───────────────────────────────────────────
                        None => {
                            // ── Fan-out path ──────────────────────────────────
                            // When the specifying handler produced multiple specs
                            // it returns them via `fan_out` instead of calling
                            // run_fanout inline, so that StateExit for the current
                            // state fires *before* any sub-spec work begins.
                            if let Some(specs) = result.fan_out {
                                run_ctx.retry_ctx = None;
                                emit(
                                    &self.event_tx,
                                    CoreEvent::ValidationPass {
                                        state: sname.into(),
                                    },
                                )
                                .await;
                                emit(
                                    &self.event_tx,
                                    CoreEvent::StateExit {
                                        state: sname.into(),
                                        outcome: Outcome::Advanced,
                                        trace_id: trace_id.to_string(),
                                        sub_spec_index: None,
                                    },
                                )
                                .await;

                                let fanout = run_fanout(
                                    &mut self.solver,
                                    specs,
                                    &run_ctx,
                                    &self.memory,
                                    run_ctx.intent.clone(),
                                    &self.event_tx,
                                )
                                .await;

                                match fanout.errors {
                                    None => {
                                        current_stage = fanout.next_stage.unwrap_or(handler_next);
                                        state = fanout.state_data;
                                    }
                                    Some(_) => {
                                        // Fan-out failed.  Handle diagnosing inline to
                                        // avoid emitting a second StateExit for sname.
                                        let (error, back) = match fanout.state_data {
                                            ProblemState::Diagnosing { error, back } => {
                                                (error, back)
                                            }
                                            _ => unreachable!(
                                                "run_fanout failure must produce Diagnosing state"
                                            ),
                                        };

                                        // Suspension short-circuit (same as main diagnosing arm).
                                        if let BackTarget::Suspend { ref questions } = back {
                                            let mut data = self
                                                .solver
                                                .take_suspension_data()
                                                .expect("solver must call store_suspension_data before returning BackTarget::Suspend");
                                            data.trace_id = trace_id.to_string();
                                            emit(
                                                &self.event_tx,
                                                CoreEvent::AwaitingHumanInput {
                                                    questions: questions.clone(),
                                                    from_state: sname.to_string(),
                                                    trace_id: trace_id.to_string(),
                                                },
                                            )
                                            .await;
                                            return Err(OrchestratorError::Suspended {
                                                questions: questions.clone(),
                                                resume_data: data,
                                                trace_id: trace_id.to_string(),
                                            });
                                        }

                                        let retry_ctx = back.retry_ctx().clone();
                                        let back_edge_reason = match retry_ctx.errors.last() {
                                            Some(e) => e.clone(),
                                            None => format!("{error}"),
                                        };
                                        match self.solver.diagnose(error, back, &run_ctx).await {
                                            Ok(recovered) => {
                                                run_ctx.retry_ctx = Some(retry_ctx);
                                                let to = state_name(&recovered);
                                                emit(
                                                    &self.event_tx,
                                                    CoreEvent::BackEdge {
                                                        from: sname.into(),
                                                        to: to.into(),
                                                        reason: back_edge_reason,
                                                        trace_id: trace_id.to_string(),
                                                    },
                                                )
                                                .await;
                                                current_stage = to;
                                                state = recovered;
                                            }
                                            Err(fatal) => {
                                                emit(
                                                    &self.event_tx,
                                                    CoreEvent::Error {
                                                        message:
                                                            "fatal error from fan-out diagnose"
                                                                .to_string(),
                                                        trace_id: trace_id.to_string(),
                                                    },
                                                )
                                                .await;
                                                // Store checkpoint for retry.
                                                // TODO: pass partial fanout results for sub-spec level retry.
                                                if let Some(cp) = self
                                                    .solver
                                                    .build_checkpoint(sname, &run_ctx, None)
                                                {
                                                    self.solver.store_suspension_data(cp);
                                                }
                                                return Err(OrchestratorError::Fatal(fatal));
                                            }
                                        }
                                    }
                                }
                                continue;
                            }

                            // ── Normal success path ───────────────────────────
                            // Update RunContext from the output of this stage.
                            match &result.state_data {
                                ProblemState::Specifying(intent) => {
                                    run_ctx.intent = Some(intent.clone());
                                }
                                ProblemState::Solving(spec) => {
                                    run_ctx.spec = Some(spec.clone());
                                }
                                _ => {}
                            }
                            // Clear retry context: this stage succeeded.
                            run_ctx.retry_ctx = None;
                            emit(
                                &self.event_tx,
                                CoreEvent::ValidationPass {
                                    state: sname.into(),
                                },
                            )
                            .await;
                            emit(
                                &self.event_tx,
                                CoreEvent::StateExit {
                                    state: sname.into(),
                                    outcome: Outcome::Advanced,
                                    trace_id: trace_id.to_string(),
                                    sub_spec_index: None,
                                },
                            )
                            .await;
                            // Explicit routing: next_stage override takes precedence
                            // over the handler's default `next` key.  Enables fan-out
                            // to jump directly to "interpreting" without relying on
                            // state_name(state_data) for dispatch.
                            current_stage = result.next_stage.unwrap_or(handler_next);
                            state = result.state_data;
                        }

                        // ── Failure ───────────────────────────────────────────
                        Some(errors) => {
                            emit(
                                &self.event_tx,
                                CoreEvent::ValidationFail {
                                    state: sname.into(),
                                    errors: errors.iter().map(|e| e.to_string()).collect(),
                                },
                            )
                            .await;

                            if errors.is_empty() {
                                // Empty-error sentinel: state_name() on state_data
                                // determines the next dispatch key.  When state_data
                                // is ProblemState::Diagnosing this routes to "diagnosing".
                                current_stage = state_name(&result.state_data);
                                state = result.state_data;
                            } else {
                                // Non-empty errors: call handler.diagnose.
                                let retry_count = revisions.get(sname).copied().unwrap_or(0);
                                let diagnose_result = match diagnose_fn {
                                    Some(ref f) => f(&errors, retry_count, result.state_data),
                                    None => Some(result.state_data),
                                };
                                match diagnose_result {
                                    Some(recovery) => {
                                        let to = state_name(&recovery);
                                        let outcome = if to == sname {
                                            Outcome::Retry
                                        } else {
                                            Outcome::BackTracked
                                        };
                                        let reason = errors
                                            .last()
                                            .map(|e| e.to_string())
                                            .unwrap_or_else(|| "back-edge".into());
                                        emit(
                                            &self.event_tx,
                                            CoreEvent::StateExit {
                                                state: sname.into(),
                                                outcome,
                                                trace_id: trace_id.to_string(),
                                                sub_spec_index: None,
                                            },
                                        )
                                        .await;
                                        emit(
                                            &self.event_tx,
                                            CoreEvent::BackEdge {
                                                from: sname.into(),
                                                to: to.into(),
                                                reason,
                                                trace_id: trace_id.to_string(),
                                            },
                                        )
                                        .await;
                                        // Set retry context so the retried stage sees
                                        // the errors that caused the back-edge.
                                        run_ctx.retry_ctx = Some(RetryContext {
                                            attempt: retry_count,
                                            errors: errors.iter().map(|e| e.to_string()).collect(),
                                            previous_output: None,
                                        });
                                        current_stage = to;
                                        state = recovery;
                                    }
                                    None => {
                                        // handler.diagnose escalated.
                                        let fatal = errors
                                            .into_iter()
                                            .next()
                                            .expect("non-empty errors on None diagnose");
                                        emit(
                                            &self.event_tx,
                                            CoreEvent::StateExit {
                                                state: sname.into(),
                                                outcome: Outcome::Failed,
                                                trace_id: trace_id.to_string(),
                                                sub_spec_index: None,
                                            },
                                        )
                                        .await;
                                        emit(
                                            &self.event_tx,
                                            CoreEvent::Error {
                                                message: "fatal error from handler diagnose"
                                                    .to_string(),
                                                trace_id: trace_id.to_string(),
                                            },
                                        )
                                        .await;
                                        // Store checkpoint for retry.
                                        if let Some(cp) =
                                            self.solver.build_checkpoint(sname, &run_ctx, None)
                                        {
                                            self.solver.store_suspension_data(cp);
                                        }
                                        return Err(OrchestratorError::Fatal(fatal));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
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

    /// Emit a StateEnter event and return the revision number used.
    async fn enter(
        tx: &Option<EventStream<Ev>>,
        sname: &'static str,
        revisions: &mut HashMap<&'static str, u32>,
        trace_id: &str,
    ) -> u32 {
        let rev = *revisions.get(sname).unwrap_or(&0);
        emit(
            tx,
            CoreEvent::StateEnter {
                state: sname.into(),
                revision: rev,
                trace_id: trace_id.to_string(),
                sub_spec_index: None,
            },
        )
        .await;
        *revisions.entry(sname).or_insert(0) += 1;
        rev
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
