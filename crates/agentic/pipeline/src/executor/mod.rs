//! [`TaskExecutor`] implementation for the agentic pipeline layer.
//!
//! This is the composition point where domain knowledge (analytics, builder,
//! workflow) meets the generic coordinator-worker infrastructure. The runtime
//! only sees [`TaskExecutor`]; this crate knows how to start the right pipeline
//! for each [`TaskSpec`] variant.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agentic_analytics::SchemaCatalog;
use agentic_builder::BuilderTestRunner;
use agentic_core::delegation::{TaskAssignment, TaskSpec};
use agentic_runtime::worker::{ExecutingTask, TaskExecutor};
use async_trait::async_trait;
use sea_orm::DatabaseConnection;

use crate::platform::{BuilderBridges, PlatformContext};
use crate::{PipelineBuilder, ThinkingMode};

// ── PipelineTaskExecutor ─────────────────────────────────────────────────────

/// Knows how to start analytics/builder pipelines and workflow executions.
///
/// Injected into the [`Worker`](agentic_runtime::worker::Worker) by the
/// HTTP/CLI layer.
pub struct PipelineTaskExecutor {
    pub platform: Arc<dyn PlatformContext>,
    /// Required for builder delegation; `None` is fine for analytics-only runs.
    pub builder_bridges: Option<BuilderBridges>,
    pub schema_cache: Option<Arc<Mutex<HashMap<String, SchemaCatalog>>>>,
    pub builder_test_runner: Option<Arc<dyn BuilderTestRunner>>,
    pub db: DatabaseConnection,
    /// Runtime state for registering answer channels (needed by workflow
    /// orchestrator tasks so the coordinator can resume them via answer channel
    /// instead of TaskSpec::Resume).
    pub state: Option<Arc<agentic_runtime::state::RuntimeState>>,
}

#[async_trait]
impl TaskExecutor for PipelineTaskExecutor {
    async fn execute(&self, assignment: TaskAssignment) -> Result<ExecutingTask, String> {
        // When this task has a parent, it's a delegation child — the
        // coordinator already created the run row, so pass the run_id
        // through to skip the duplicate insert.
        let is_child = assignment.parent_task_id.is_some();
        match &assignment.spec {
            TaskSpec::Agent { agent_id, question } => {
                self.execute_agent(
                    agent_id,
                    question,
                    if is_child {
                        Some(assignment.run_id.clone())
                    } else {
                        None
                    },
                )
                .await
            }

            TaskSpec::Workflow {
                workflow_ref,
                variables,
            } => {
                self.execute_workflow(&assignment.run_id, workflow_ref, variables.clone())
                    .await
            }

            TaskSpec::Resume {
                run_id,
                resume_data,
                answer,
            } => {
                self.execute_resume(run_id, resume_data.clone(), answer.clone())
                    .await
            }

            TaskSpec::WorkflowStep {
                step_config,
                render_context,
                workflow_context,
            } => {
                self.execute_workflow_step(
                    step_config.clone(),
                    render_context.clone(),
                    workflow_context.clone(),
                )
                .await
            }

            TaskSpec::WorkflowDecision {
                run_id,
                pending_child_answer,
            } => {
                self.execute_workflow_decision(run_id, pending_child_answer.clone())
                    .await
            }
        }
    }

    async fn resume_from_state(
        &self,
        run: &agentic_runtime::entity::run::Model,
        suspend_data: Option<agentic_core::human_input::SuspendedRunData>,
    ) -> Result<ExecutingTask, String> {
        let source_type = run.source_type.as_deref().unwrap_or("analytics");

        // Temporal-style workflow runs: if `agentic_workflow_state` exists for
        // this run, resume by enqueuing a WorkflowDecision (stateless path).
        if source_type == "workflow" {
            match agentic_workflow::extension::load_workflow_state(&self.db, &run.id).await {
                Ok(Some(_)) => {
                    return self.execute_workflow_decision(&run.id, None).await;
                }
                Ok(None) => {
                    // No durable state (run started before the Temporal refactor).
                    // Fall through to legacy resume path below.
                }
                Err(e) => {
                    tracing::warn!(
                        target: "pipeline",
                        run_id = %run.id,
                        error = %e,
                        "failed to check workflow state; falling back to legacy resume"
                    );
                }
            }
        }

        // Also check task_metadata for workflow orchestrator state.
        if let Some(ref meta) = run.task_metadata
            && meta.get("original_spec").is_some()
            && let Some(spec) = meta.get("original_spec")
            && spec.get("type").and_then(|t| t.as_str()) == Some("workflow")
        {
            // This was a workflow child — try to re-run the workflow.
            if let Some(workflow_ref) = spec.get("workflow_ref").and_then(|v| v.as_str()) {
                return self.execute_workflow(&run.id, workflow_ref, None).await;
            }
        }

        match source_type {
            "workflow" | "workflow_step" => {
                // Workflow tasks without orchestrator checkpoint.
                if let Some(data) = suspend_data {
                    self.execute_resume(&run.id, data, String::new()).await
                } else {
                    Err(format!(
                        "cannot resume workflow run {}: no saved state",
                        run.id
                    ))
                }
            }
            _ => {
                // Analytics/builder: resume from checkpoint if available.
                if let Some(data) = suspend_data {
                    self.execute_resume(&run.id, data, String::new()).await
                } else {
                    // No checkpoint — run hadn't reached a suspension point.
                    // Cannot resume; user needs to resubmit the question.
                    Err(format!(
                        "run {} (type={source_type}) has no checkpoint — resubmit the question",
                        run.id
                    ))
                }
            }
        }
    }
}

/// The well-known agent ID that routes to the builder domain instead of
/// analytics.  Used by analytics → builder delegation.
pub const BUILDER_AGENT_ID: &str = "__builder__";

/// Returns `true` when `agent_id` should be routed to the builder domain
/// rather than the analytics domain.
fn is_builder_agent(agent_id: &str) -> bool {
    agent_id == BUILDER_AGENT_ID
}

impl PipelineTaskExecutor {
    async fn execute_agent(
        &self,
        agent_id: &str,
        question: &str,
        existing_run_id: Option<String>,
    ) -> Result<ExecutingTask, String> {
        let mut pb = PipelineBuilder::new(self.platform.clone());
        if let Some(bridges) = self.builder_bridges.clone() {
            pb = pb.with_builder_bridges(bridges);
        }
        let mut builder = if is_builder_agent(agent_id) {
            pb.builder(None)
        } else {
            pb.analytics(agent_id)
        }
        .question(question)
        .thinking_mode(ThinkingMode::Auto);

        // For delegation children, use the coordinator-assigned run_id
        // and skip the duplicate DB insert.
        if let Some(run_id) = existing_run_id.clone() {
            builder = builder.existing_run(run_id);
        }

        // Auto-accept propose_change when builder runs as a delegation child
        // (existing_run_id is set → the coordinator created this task).
        if is_builder_agent(agent_id) && existing_run_id.is_some() {
            builder = builder.human_input(std::sync::Arc::new(
                agentic_core::human_input::AutoAcceptInputProvider,
            ));
        }

        if let Some(cache) = &self.schema_cache {
            builder = builder.schema_cache(cache.clone());
        }
        if let Some(runner) = &self.builder_test_runner {
            builder = builder.test_runner(runner.clone());
        }

        let started = builder
            .start(&self.db)
            .await
            .map_err(|e| format!("failed to start agent pipeline: {e}"))?;

        let (task, _bridge) = started.into_executing_task();
        Ok(task)
    }

    async fn execute_resume(
        &self,
        run_id: &str,
        resume_data: agentic_core::human_input::SuspendedRunData,
        answer: String,
    ) -> Result<ExecutingTask, String> {
        // Load run from DB to get source_type, agent_id, model, thread_id.
        let run = agentic_runtime::crud::get_run(&self.db, run_id)
            .await
            .map_err(|e| format!("failed to load run: {e}"))?
            .ok_or_else(|| format!("run {run_id} not found"))?;

        let source_type = run.source_type.as_deref().unwrap_or("analytics");
        let agent_id = run
            .metadata
            .as_ref()
            .and_then(|m| m.get("agent_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_default();
        let model = run
            .metadata
            .as_ref()
            .and_then(|m| m.get("model"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut builder = PipelineBuilder::new(self.platform.clone()).question(&run.question);
        if let Some(bridges) = self.builder_bridges.clone() {
            builder = builder.with_builder_bridges(bridges);
        }

        if let Some(cache) = &self.schema_cache {
            builder = builder.schema_cache(cache.clone());
        }
        if let Some(runner) = &self.builder_test_runner {
            builder = builder.test_runner(runner.clone());
        }
        if let Some(tid) = run.thread_id {
            builder = builder.thread(tid);
        }

        let started = builder
            .resume(
                &self.db,
                run_id,
                source_type,
                &agent_id,
                model,
                resume_data,
                answer,
            )
            .await
            .map_err(|e| format!("failed to resume pipeline: {e}"))?;

        let (task, _bridge) = started.into_executing_task();
        Ok(task)
    }
}

mod workflow;

pub use workflow::run_decision_task;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_agent_id_routes_to_builder() {
        assert!(is_builder_agent("__builder__"));
    }

    #[test]
    fn regular_agent_id_routes_to_analytics() {
        assert!(!is_builder_agent("revenue"));
        assert!(!is_builder_agent("duckdb"));
        assert!(!is_builder_agent(""));
    }
}
