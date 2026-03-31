//! `DomainSolver<AppBuilderDomain>` implementation for [`AppBuilderSolver`].

use std::sync::Arc;

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

use crate::tools::{clarifying_tools, solving_tools, specifying_tools};
use crate::types::{
    AppAnswer, AppBuilderDomain, AppBuilderError, AppIntent, AppResult, AppSolution, AppSpec,
};

use super::{AppBuilderSolver, diagnosing, resuming};

// ---------------------------------------------------------------------------
// DomainSolver impl
// ---------------------------------------------------------------------------

#[async_trait]
impl DomainSolver<AppBuilderDomain> for AppBuilderSolver {
    fn tools_for_state(state: &str) -> Vec<ToolDef> {
        match state {
            "clarifying" => clarifying_tools(),
            "specifying" => specifying_tools(),
            "solving" => solving_tools(),
            _ => vec![],
        }
    }

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
            .expect("AppBuilderSolver must have a default connector");

        match state {
            "clarifying" => {
                crate::tools::execute_clarifying_tool_with_connector(
                    name,
                    params,
                    &self.catalog,
                    connector.as_ref(),
                )
                .await
            }
            "specifying" => {
                crate::tools::execute_specifying_tool(
                    name,
                    params,
                    &self.catalog,
                    connector.as_ref(),
                )
                .await
            }
            "solving" => crate::tools::execute_solving_tool(name, params, connector.as_ref()).await,
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
        _memory: &SessionMemory<AppBuilderDomain>,
    ) -> Option<ProblemState<AppBuilderDomain>> {
        Some(resuming::problem_state_from_resume(data))
    }

    // ── Pipeline stage delegates ──────────────────────────────────────────────

    async fn clarify(
        &mut self,
        intent: AppIntent,
        _ctx: &RunContext<AppBuilderDomain>,
        _memory: &SessionMemory<AppBuilderDomain>,
    ) -> Result<AppIntent, (AppBuilderError, BackTarget<AppBuilderDomain>)> {
        self.clarify_impl(intent).await
    }

    async fn specify(
        &mut self,
        intent: AppIntent,
        _ctx: &RunContext<AppBuilderDomain>,
        _memory: &SessionMemory<AppBuilderDomain>,
    ) -> Result<Vec<AppSpec>, (AppBuilderError, BackTarget<AppBuilderDomain>)> {
        // Retry path: return pre-computed specs without calling the LLM.
        if let Some(specs) = self.pre_computed_specs.take() {
            return Ok(specs);
        }

        let spec = self.specify_impl(intent).await?;

        // Fan out: split into one AppSpec per task so each task gets its own
        // solve→execute sub-spec tracked independently by the orchestrator.
        let per_task_specs: Vec<AppSpec> = spec
            .tasks
            .iter()
            .map(|task| AppSpec {
                intent: spec.intent.clone(),
                app_name: spec.app_name.clone(),
                description: spec.description.clone(),
                tasks: vec![task.clone()],
                controls: spec.controls.clone(),
                layout: spec.layout.clone(),
                connector_name: spec.connector_name.clone(),
            })
            .collect();

        Ok(per_task_specs)
    }

    fn fanout_worker<Ev: DomainEvents>(
        &self,
    ) -> Option<Arc<dyn FanoutWorker<AppBuilderDomain, Ev>>> {
        Some(self.build_fanout_worker())
    }

    fn merge_results(&self, results: Vec<AppResult>) -> Result<AppResult, AppBuilderError> {
        // Take controls and layout from the first result (they are identical
        // across all per-task results).
        let first = results.first().expect("at least one result");
        let controls = first.controls.clone();
        let layout = first.layout.clone();
        let connector_name = first.connector_name.clone();

        let task_results = results.into_iter().flat_map(|r| r.task_results).collect();

        Ok(AppResult {
            task_results,
            controls,
            layout,
            connector_name,
        })
    }

    async fn solve(
        &mut self,
        spec: AppSpec,
        ctx: &RunContext<AppBuilderDomain>,
        _memory: &SessionMemory<AppBuilderDomain>,
    ) -> Result<AppSolution, (AppBuilderError, BackTarget<AppBuilderDomain>)> {
        let retry_error = ctx
            .retry_ctx
            .as_ref()
            .and_then(|r| r.errors.first())
            .filter(|s| !s.is_empty())
            .cloned();
        self.solve_impl(spec, retry_error).await
    }

    async fn execute(
        &mut self,
        solution: AppSolution,
        _ctx: &RunContext<AppBuilderDomain>,
        _memory: &SessionMemory<AppBuilderDomain>,
    ) -> Result<AppResult, (AppBuilderError, BackTarget<AppBuilderDomain>)> {
        self.execute_impl(solution).await
    }

    async fn interpret(
        &mut self,
        result: AppResult,
        _ctx: &RunContext<AppBuilderDomain>,
        _memory: &SessionMemory<AppBuilderDomain>,
    ) -> Result<AppAnswer, (AppBuilderError, BackTarget<AppBuilderDomain>)> {
        self.interpret_impl(result).await
    }

    async fn diagnose(
        &mut self,
        error: AppBuilderError,
        back: BackTarget<AppBuilderDomain>,
        ctx: &RunContext<AppBuilderDomain>,
    ) -> Result<ProblemState<AppBuilderDomain>, AppBuilderError> {
        diagnosing::diagnose_impl(error, back, ctx).await
    }

    // ── Retry-from-checkpoint ─────────────────────────────────────────────────

    fn build_checkpoint(
        &self,
        failed_state: &str,
        ctx: &RunContext<AppBuilderDomain>,
        partial_fanout: Option<&[(usize, bool)]>,
    ) -> Option<SuspendedRunData> {
        let mut stage_data = serde_json::json!({ "checkpoint_type": "retry" });
        if let Some(intent) = &ctx.intent {
            stage_data["intent"] = serde_json::to_value(intent).ok()?;
        }
        if let Some(spec) = &ctx.spec {
            stage_data["spec"] = serde_json::to_value(spec).ok()?;
        }
        if let Some(fanout) = partial_fanout {
            stage_data["fanout_status"] = serde_json::to_value(fanout).ok()?;
        }
        Some(SuspendedRunData {
            from_state: failed_state.to_string(),
            original_input: ctx
                .intent
                .as_ref()
                .map(|i| i.raw_request.clone())
                .unwrap_or_default(),
            trace_id: String::new(),
            stage_data,
            question: String::new(),
            suggestions: vec![],
        })
    }
}
