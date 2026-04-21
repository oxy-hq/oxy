use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    back_target::BackTarget,
    domain::Domain,
    events::{DomainEvents, EventStream},
    human_input::{ResumeInput, SuspendedRunData},
    orchestrator::{RunContext, SessionMemory},
    state::ProblemState,
    tools::{ToolDef, ToolError},
};

// ── FanoutWorker ─────────────────────────────────────────────────────────────

/// A lightweight, `Send + Sync` worker that can solve and execute a single
/// spec concurrently with other workers during a fan-out.
///
/// The orchestrator spawns one task per spec; each task clones this `Arc` and
/// calls [`solve_and_execute`].  Implementations should hold only shared
/// (`Arc`-wrapped) or cloneable state — never `&mut self`.
///
/// [`solve_and_execute`]: FanoutWorker::solve_and_execute
#[async_trait]
pub trait FanoutWorker<D: Domain, Ev: DomainEvents>: Send + Sync {
    /// Solve **and** execute a single spec.
    ///
    /// `index` and `total` identify this spec within the fan-out group.
    /// Implementations should tag all emitted events with
    /// `sub_spec_index = Some(index)`.
    async fn solve_and_execute(
        &self,
        spec: D::Spec,
        index: usize,
        total: usize,
        events: &Option<EventStream<Ev>>,
        ctx: &RunContext<D>,
        mem: &SessionMemory<D>,
    ) -> Result<D::Result, (D::Error, BackTarget<D>)>;
}

/// The async pipeline solver for a given domain.
///
/// Each method corresponds to one stage of the pipeline.  On success a method
/// returns the data required by the *next* stage; on failure it returns a
/// `(error, BackTarget)` pair which the orchestrator uses to enter
/// [`ProblemState::Diagnosing`].
///
/// The `diagnose` method is called whenever a `Diagnosing` state is reached.
/// It either returns a new [`ProblemState`] to resume from, or propagates a
/// fatal error that terminates the run.
///
/// # State skipping
///
/// Two mechanisms allow a domain to bypass pipeline stages:
///
/// - **Static** (`SKIP_STATES`): a `const` listing stages this domain never
///   uses.  The orchestrator seeds its internal skip set from this value.
///   Domains that declare a static skip **must** also override `should_skip`
///   to supply the data transformation (since the orchestrator cannot derive
///   the next state's data generically).
///
/// - **Dynamic** (`should_skip`): called by the orchestrator before every
///   worker state.  Return `Some(next_state)` to skip the current state and
///   advance directly to `next_state` without calling `execute` or emitting
///   `StateEnter`/`StateExit` events.  Return `None` to execute normally.
///
/// # Implementing `solve` with back-edges
///
/// When `solve` needs to construct a `BackTarget::Specify` back-edge, it can
/// use the `ctx: &RunContext<D>` parameter to access the originating intent
/// (stored in `ctx.intent` after the Clarifying stage).
///
/// [`ProblemState::Diagnosing`]: crate::state::ProblemState::Diagnosing
/// [`ProblemState`]: crate::state::ProblemState
#[async_trait]
pub trait DomainSolver<D: Domain>: Send + Sync {
    /// States this domain statically never uses.
    ///
    /// The orchestrator seeds its internal skip set from this constant.
    /// Domains that list a state here **must** override [`should_skip`] to
    /// return `Some(next_state)` for those states; otherwise the orchestrator
    /// falls through and executes them normally.
    ///
    /// [`should_skip`]: DomainSolver::should_skip
    const SKIP_STATES: &'static [&'static str] = &[];
    /// Refine or validate the initial intent.
    ///
    /// Returns the (possibly updated) intent, or an error with a back-target.
    async fn clarify(
        &mut self,
        intent: D::Intent,
        ctx: &RunContext<D>,
        memory: &SessionMemory<D>,
    ) -> Result<D::Intent, (D::Error, BackTarget<D>)>;

    /// Produce one or more structured [`Spec`]s from the clarified intent.
    ///
    /// Returns a `Vec` to support fan-out: multiple specs means the
    /// orchestrator will solve/execute each independently. The default
    /// implementation delegates to [`specify_single`] and wraps the result
    /// in a single-element `Vec`.
    ///
    /// Override this method (not `specify_single`) to enable fan-out.
    ///
    /// [`Spec`]: Domain::Spec
    /// [`specify_single`]: DomainSolver::specify_single
    async fn specify(
        &mut self,
        intent: D::Intent,
        ctx: &RunContext<D>,
        memory: &SessionMemory<D>,
    ) -> Result<Vec<D::Spec>, (D::Error, BackTarget<D>)> {
        self.specify_single(intent, ctx, memory)
            .await
            .map(|s| vec![s])
    }

    /// Produce exactly one [`Spec`] from the clarified intent.
    ///
    /// This is the common case. Override this for your domain's specifying
    /// logic. The default [`specify`] wraps the result in a single-element
    /// `Vec`.
    ///
    /// Solvers that override [`specify`] directly for fan-out do not need to
    /// implement this method; the default body panics to make accidental
    /// calls obvious.
    ///
    /// [`Spec`]: Domain::Spec
    /// [`specify`]: DomainSolver::specify
    async fn specify_single(
        &mut self,
        _intent: D::Intent,
        _ctx: &RunContext<D>,
        _memory: &SessionMemory<D>,
    ) -> Result<D::Spec, (D::Error, BackTarget<D>)> {
        unimplemented!(
            "specify_single must be implemented, or specify() must be overridden for fan-out"
        )
    }

    /// Derive a concrete [`Solution`] from the spec.
    ///
    /// Use `ctx.intent` to access the originating intent when constructing
    /// a `BackTarget::Specify` back-edge.
    ///
    /// [`Solution`]: Domain::Solution
    async fn solve(
        &mut self,
        spec: D::Spec,
        ctx: &RunContext<D>,
        memory: &SessionMemory<D>,
    ) -> Result<D::Solution, (D::Error, BackTarget<D>)>;

    /// Execute the solution and capture raw output.
    async fn execute(
        &mut self,
        solution: D::Solution,
        ctx: &RunContext<D>,
        memory: &SessionMemory<D>,
    ) -> Result<D::Result, (D::Error, BackTarget<D>)>;

    /// Interpret raw output into a user-facing answer.
    async fn interpret(
        &mut self,
        result: D::Result,
        ctx: &RunContext<D>,
        memory: &SessionMemory<D>,
    ) -> Result<D::Answer, (D::Error, BackTarget<D>)>;

    /// Optionally skip the current state without executing it.
    ///
    /// Called by the orchestrator before every worker state.  Return
    /// `Some(next_state)` to bypass this state entirely — no `StateEnter` or
    /// `StateExit` events will be emitted and the handler's `execute` function
    /// will not be called.  Return `None` to execute normally.
    ///
    /// The default implementation always returns `None`.
    ///
    /// # Example — dynamic skip based on spec data
    ///
    /// ```ignore
    /// fn should_skip(&mut self, state: &str, data: &ProblemState<D>)
    ///     -> Option<ProblemState<D>>
    /// {
    ///     if state == "solving" {
    ///         if let ProblemState::Solving(spec) = data {
    ///             if spec.solution_source == SolutionSource::SemanticLayer {
    ///                 let sql = self.pre_compiled_sql.take().unwrap_or_default();
    ///                 return Some(ProblemState::Executing(Solution { sql }));
    ///             }
    ///         }
    ///     }
    ///     None
    /// }
    /// ```
    fn should_skip(
        &mut self,
        state: &str,
        data: &ProblemState<D>,
        run_ctx: &RunContext<D>,
    ) -> Option<ProblemState<D>> {
        let _ = (state, data, run_ctx);
        None
    }

    /// Diagnose an error and return the state to jump to for recovery.
    ///
    /// `ctx` gives access to the accumulated prior-stage outputs (intent, spec,
    /// retry context) so the solver can route back-edges without storing state.
    ///
    /// Returns `Ok(state)` to resume the pipeline at the given state, or
    /// `Err(fatal)` to abort the entire run with a fatal error.
    async fn diagnose(
        &mut self,
        error: D::Error,
        back: BackTarget<D>,
        ctx: &RunContext<D>,
    ) -> Result<ProblemState<D>, D::Error>;

    /// Return the tools available to the LLM for the given pipeline state.
    ///
    /// Called by state handlers before invoking the LLM so only the tools
    /// relevant to that stage are surfaced.  The default returns an empty
    /// `Vec` — states that don't use tools need not override this.
    fn tools_for_state(state: &str) -> Vec<ToolDef> {
        let _ = state;
        vec![]
    }

    /// Execute a named tool with the given JSON parameters.
    ///
    /// Called by state handlers after the LLM returns a tool-use response.
    /// The `state` argument identifies which state's tool set is active so
    /// the implementation can dispatch to the correct executor.
    async fn execute_tool(
        &mut self,
        state: &str,
        name: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, ToolError> {
        let _ = (state, name, params);
        Err(ToolError::UnknownTool("no tools registered".into()))
    }

    /// Combine raw results from independent spec executions.
    ///
    /// Called by the orchestrator when [`specify`] returns multiple specs.
    /// Domains must implement this if they override `specify` to return
    /// multiple specs. The default panics.
    ///
    /// [`specify`]: DomainSolver::specify
    fn merge_results(&self, results: Vec<D::Result>) -> Result<D::Result, D::Error> {
        let _ = results;
        unimplemented!("merge_results must be implemented for multi-spec fan-out")
    }

    /// Return an independent worker that can solve+execute a single spec
    /// concurrently with other workers.
    ///
    /// When `Some`, `run_fanout` spawns concurrent tasks instead of iterating
    /// sequentially.  When `None` (the default), the existing serial fan-out
    /// path is used.
    fn fanout_worker<Ev: DomainEvents>(&self) -> Option<Arc<dyn FanoutWorker<D, Ev>>> {
        None
    }

    /// Maximum number of retries per sub-spec during concurrent fan-out.
    ///
    /// When a sub-spec's solve+execute fails, the fanout worker retries it
    /// with the error message injected into [`RetryContext`] so the LLM can
    /// correct its approach.  Only the failed sub-spec is retried; successful
    /// ones are kept.
    ///
    /// Default is 2 (up to 3 total attempts per sub-spec).
    ///
    /// [`RetryContext`]: crate::back_target::RetryContext
    fn max_fanout_retries(&self) -> u32 {
        2
    }

    // ── Human-in-the-loop hooks ───────────────────────────────────────────────

    /// Persist suspension data so the orchestrator can retrieve it after a
    /// [`BackTarget::Suspend`] is returned.
    ///
    /// Call this **before** returning `Err((_, BackTarget::Suspend { .. }))`.
    /// The orchestrator calls [`take_suspension_data`] immediately after
    /// detecting the `Suspend` back-edge to assemble
    /// [`OrchestratorError::Suspended`].
    ///
    /// Default implementation is a no-op.  Solvers that use `ask_user` must
    /// override both this method and [`take_suspension_data`].
    ///
    /// [`BackTarget::Suspend`]: crate::back_target::BackTarget::Suspend
    /// [`take_suspension_data`]: DomainSolver::take_suspension_data
    /// [`OrchestratorError::Suspended`]: crate::orchestrator::OrchestratorError::Suspended
    fn store_suspension_data(&mut self, _data: SuspendedRunData) {}

    /// Retrieve and clear the previously stored suspension data.
    ///
    /// Called by the orchestrator when it detects a [`BackTarget::Suspend`]
    /// back-edge.  Returns `None` if no suspension data has been stored.
    ///
    /// Default implementation always returns `None`.
    ///
    /// [`BackTarget::Suspend`]: crate::back_target::BackTarget::Suspend
    fn take_suspension_data(&mut self) -> Option<SuspendedRunData> {
        None
    }

    /// Inject resume input before the orchestrator re-enters the pipeline.
    ///
    /// Called by [`Orchestrator::resume`] before calling `run_pipeline_inner`.
    /// The solver should store this and consume it (via `take`) at the start
    /// of the relevant `_impl` method to build the synthetic message history.
    ///
    /// Default implementation is a no-op.
    ///
    /// [`Orchestrator::resume`]: crate::orchestrator::Orchestrator::resume
    fn set_resume_data(&mut self, _data: ResumeInput) {}

    /// Build the starting [`ProblemState`] for a resume run.
    ///
    /// Called by [`Orchestrator::resume`] to determine which FSM state to
    /// re-enter.  Uses `data.from_state` and `data.stage_data` to reconstruct
    /// the domain type required by that state.
    ///
    /// Return `Some(state)` to resume the pipeline at that state, or `None`
    /// if this solver does not support HITL resumption.  The orchestrator
    /// treats `None` as a fatal error (the suspended pipeline cannot be
    /// recovered) rather than panicking.
    ///
    /// The default implementation returns `None` — solvers that support
    /// `ask_user` must override this.
    ///
    /// [`Orchestrator::resume`]: crate::orchestrator::Orchestrator::resume
    fn problem_state_from_resume(
        &self,
        _data: &SuspendedRunData,
        _memory: &SessionMemory<D>,
    ) -> Option<ProblemState<D>> {
        None
    }

    /// Optionally populate the [`RunContext`] when resuming from a suspension.
    ///
    /// Called by [`Orchestrator::resume`] after [`problem_state_from_resume`]
    /// but before re-entering the pipeline.  Override this when the resumed
    /// state skips earlier stages (e.g. jumping straight to Interpreting from
    /// Executing) and the `RunContext` fields (`intent`, `spec`) would
    /// otherwise be `None`.
    ///
    /// The default implementation does nothing (leaves RunContext empty).
    fn populate_resume_context(&self, _data: &SuspendedRunData, _run_ctx: &mut RunContext<D>) {}

    // ── Retry-from-checkpoint hooks ───────────────────────────────────────────

    /// Build checkpoint data so a failed run can be retried from this point.
    ///
    /// Called by the orchestrator before returning
    /// [`OrchestratorError::Fatal`].  The orchestrator passes the current
    /// `RunContext` (accumulated intent + spec) and, for fan-out failures,
    /// a list of `(sub_spec_index, succeeded)` pairs.
    ///
    /// Return `Some(data)` to enable retry; the orchestrator will persist
    /// it via [`store_suspension_data`] so it can be retrieved later with
    /// [`take_suspension_data`].  Return `None` if the domain does not
    /// support retry.
    ///
    /// The default implementation returns `None`.
    ///
    /// [`OrchestratorError::Fatal`]: crate::orchestrator::OrchestratorError::Fatal
    /// [`store_suspension_data`]: DomainSolver::store_suspension_data
    /// [`take_suspension_data`]: DomainSolver::take_suspension_data
    fn build_checkpoint(
        &self,
        _failed_state: &str,
        _ctx: &RunContext<D>,
        _partial_fanout: Option<&[(usize, bool)]>,
    ) -> Option<SuspendedRunData> {
        None
    }
}
