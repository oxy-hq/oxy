use async_trait::async_trait;

use agentic_core::events::DomainEvents;
use agentic_core::solver::FanoutWorker;
use agentic_core::tools::{ToolDef, ToolError};
use agentic_core::{
    back_target::BackTarget,
    human_input::{ResumeInput, SuspendedRunData},
    orchestrator::{RunContext, SessionMemory},
    solver::DomainSolver,
    state::ProblemState,
};

use std::sync::{Arc, Mutex};

use crate::tools::{
    clarifying_tools, execute_clarifying_tool, execute_interpreting_tool, execute_solving_tool,
    execute_specifying_tool, interpreting_tools, solving_tools, specifying_tools,
};
use crate::types::DisplayBlock;
use crate::{
    AnalyticsAnswer, AnalyticsDomain, AnalyticsError, AnalyticsIntent, AnalyticsResult,
    AnalyticsSolution, QuerySpec,
};

use super::AnalyticsSolver;
use super::diagnosing;
use super::resuming::{self, ask_user_tool_def};

// ---------------------------------------------------------------------------
// DomainSolver impl
// ---------------------------------------------------------------------------

#[async_trait]
impl DomainSolver<AnalyticsDomain> for AnalyticsSolver {
    /// Return the tool list for a state.
    ///
    /// NOTE: `ask_user` is listed for clarifying and specifying so the LLM can
    /// invoke it, but it is **intercepted inside the tool loop** before this
    /// function's caller reaches `execute_tool` — see `resuming.rs` for details
    /// (smell #1 fix: documented here to avoid confusion).
    fn tools_for_state(state: &str) -> Vec<ToolDef> {
        match state {
            "clarifying" => {
                let mut tools = clarifying_tools(false);
                tools.push(ask_user_tool_def());
                tools
            }
            "specifying" => {
                let mut tools = specifying_tools(false);
                tools.push(ask_user_tool_def());
                tools
            }
            "solving" => solving_tools(),
            "interpreting" => interpreting_tools(),
            _ => vec![],
        }
    }

    /// Dispatch a tool call to the correct per-state executor.
    ///
    /// **NOTE:** This is the fallback for the default orchestrator handler.
    /// When custom `StateHandler`s are active (via `build_analytics_handlers`),
    /// each handler wires its own tool dispatch closure, so this method is
    /// NOT called for those states (smell #2 fix: documented to avoid confusion).
    async fn execute_tool(
        &mut self,
        state: &str,
        name: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, ToolError> {
        let connector = self
            .connectors
            .get(&self.default_connector)
            .cloned()
            .expect("default connector must be registered");
        match state {
            "clarifying" => {
                if name == "search_procedures" {
                    let query = params["query"].as_str().unwrap_or("");
                    let refs = match self.procedure_runner.as_ref() {
                        Some(runner) => runner.search(query).await,
                        None => vec![],
                    };
                    let items: Vec<serde_json::Value> = refs
                        .iter()
                        .map(|r| {
                            serde_json::json!({
                                "name": r.name,
                                "path": r.path.display().to_string(),
                                "description": r.description,
                            })
                        })
                        .collect();
                    Ok(serde_json::json!({ "procedures": items }))
                } else {
                    execute_clarifying_tool(name, params, &*self.catalog)
                }
            }
            "specifying" => {
                execute_specifying_tool(name, params, &*self.catalog, &*connector).await
            }
            "solving" => execute_solving_tool(name, params, &*connector).await,
            // NOTE: this fallback is never reached in production — the custom
            // interpreting StateHandler wires its own closure with the real
            // event_tx, result_sets, and valid_charts captured from interpret_impl.
            "interpreting" => {
                execute_interpreting_tool(
                    name,
                    params,
                    &None,
                    &[],
                    &Arc::new(Mutex::new(Vec::<DisplayBlock>::new())),
                )
                .await
            }
            _ => Err(ToolError::UnknownTool(format!(
                "no tools for state '{state}'"
            ))),
        }
    }

    // ── HITL hooks ────────────────────────────────────────────────────────────

    fn store_suspension_data(&mut self, data: SuspendedRunData) {
        self.suspension_data = Some(data);
    }

    fn take_suspension_data(&mut self) -> Option<SuspendedRunData> {
        self.suspension_data.take()
    }

    fn set_resume_data(&mut self, data: ResumeInput) {
        self.resume_data = Some(data);
    }

    fn problem_state_from_resume(
        &self,
        data: &SuspendedRunData,
        _memory: &SessionMemory<AnalyticsDomain>,
    ) -> Option<ProblemState<AnalyticsDomain>> {
        let answer = self.resume_data.as_ref().map(|r| r.answer.as_str());
        Some(resuming::problem_state_from_resume(data, answer))
    }

    fn populate_resume_context(
        &self,
        data: &SuspendedRunData,
        run_ctx: &mut RunContext<AnalyticsDomain>,
    ) {
        resuming::populate_resume_context(data, run_ctx);
    }

    // ── Pipeline stage delegates ──────────────────────────────────────────────

    async fn clarify(
        &mut self,
        intent: AnalyticsIntent,
        _ctx: &RunContext<AnalyticsDomain>,
        memory: &SessionMemory<AnalyticsDomain>,
    ) -> Result<AnalyticsIntent, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
        use crate::solver::clarifying::ClarifyOutcome;
        match self
            .clarify_impl(intent.clone(), _ctx.retry_ctx.as_ref(), memory.turns())
            .await?
        {
            ClarifyOutcome::Intent(clarified) => Ok(clarified),
            // The semantic shortcut produces a solution directly; the legacy
            // DomainSolver trait path cannot express this, so fall back to
            // forwarding the original intent to Specifying (which will
            // re-compile via the normal path).
            ClarifyOutcome::SemanticShortcut(_) => Ok(intent),
        }
    }

    async fn specify(
        &mut self,
        intent: AnalyticsIntent,
        ctx: &RunContext<AnalyticsDomain>,
        _memory: &SessionMemory<AnalyticsDomain>,
    ) -> Result<Vec<QuerySpec>, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
        self.specify_impl(intent, ctx.retry_ctx.as_ref()).await
    }

    fn merge_results(
        &self,
        results: Vec<AnalyticsResult>,
    ) -> Result<AnalyticsResult, AnalyticsError> {
        Ok(AnalyticsResult {
            results: results.into_iter().flat_map(|r| r.results).collect(),
        })
    }

    fn fanout_worker<Ev: DomainEvents>(
        &self,
    ) -> Option<Arc<dyn FanoutWorker<AnalyticsDomain, Ev>>> {
        Some(Arc::new(
            super::fanout_worker::AnalyticsFanoutWorker::from_solver(self),
        ))
    }

    async fn solve(
        &mut self,
        spec: QuerySpec,
        ctx: &RunContext<AnalyticsDomain>,
        _memory: &SessionMemory<AnalyticsDomain>,
    ) -> Result<AnalyticsSolution, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
        self.solve_impl(spec, ctx.retry_ctx.as_ref()).await
    }

    async fn execute(
        &mut self,
        solution: AnalyticsSolution,
        _ctx: &RunContext<AnalyticsDomain>,
        _memory: &SessionMemory<AnalyticsDomain>,
    ) -> Result<AnalyticsResult, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
        self.execute_solution(solution).await
    }

    async fn interpret(
        &mut self,
        result: AnalyticsResult,
        ctx: &RunContext<AnalyticsDomain>,
        memory: &SessionMemory<AnalyticsDomain>,
    ) -> Result<AnalyticsAnswer, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
        // Extract question, history, and question_type from the spec stored in
        // ctx (populated by the Specifying stage).
        let (raw_question, history, question_type) = ctx
            .spec
            .as_ref()
            .map(|s| {
                (
                    s.intent.raw_question.clone(),
                    s.intent.history.clone(),
                    Some(s.intent.question_type.clone()),
                )
            })
            .unwrap_or_default();
        self.interpret_impl(
            &raw_question,
            &history,
            result,
            memory.turns(),
            question_type.as_ref(),
        )
        .await
    }

    /// Solving is absorbed into the specifying handler — no skip logic needed.
    /// The specifying handler transitions directly to Executing.

    async fn diagnose(
        &mut self,
        error: AnalyticsError,
        back: BackTarget<AnalyticsDomain>,
        ctx: &RunContext<AnalyticsDomain>,
    ) -> Result<ProblemState<AnalyticsDomain>, AnalyticsError> {
        diagnosing::diagnose_impl(error, back, ctx).await
    }
}
