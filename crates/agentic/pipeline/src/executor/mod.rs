//! [`TaskExecutor`] implementation for the agentic pipeline layer.
//!
//! This is the composition point where domain knowledge (analytics, builder,
//! workflow) meets the generic coordinator-worker infrastructure. The runtime
//! only sees [`TaskExecutor`]; this crate knows how to start the right pipeline
//! for each [`TaskSpec`] variant.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agentic_analytics::SchemaCatalog;
use agentic_builder::{BuilderAppRunner, BuilderTestRunner};
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
    pub builder_app_runner: Option<Arc<dyn BuilderAppRunner>>,
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
            TaskSpec::Agent {
                agent_id,
                question,
                extra,
            } => {
                // Top-level scheduled agent runs pre-seed the run row in
                // `start_agent_run` (mirrors the workflow / airway
                // pattern). Detect that case by checking the DB and pass
                // `existing_run_id` so the analytics builder doesn't try
                // to insert a duplicate row.
                let pre_seeded = !is_child
                    && agentic_runtime::crud::get_run(&self.db, &assignment.run_id)
                        .await
                        .map_err(|e| format!("failed to load run: {e}"))?
                        .is_some();
                let existing_run_id = if is_child || pre_seeded {
                    Some(assignment.run_id.clone())
                } else {
                    None
                };
                self.execute_agent(agent_id, question, existing_run_id, extra.as_ref())
                    .await
            }

            TaskSpec::Workflow {
                workflow_ref,
                variables,
                retry_from_run_id,
                cache_enabled,
                body,
                initial_render_context,
            } => {
                self.execute_workflow(
                    &assignment.run_id,
                    workflow_ref,
                    variables.clone(),
                    retry_from_run_id.clone(),
                    *cache_enabled,
                    body.clone(),
                    initial_render_context.clone(),
                )
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

            TaskSpec::Custom { kind, .. } => Err(format!(
                "PipelineTaskExecutor does not handle Custom tasks (kind: {kind})"
            )),

            TaskSpec::Airway {
                pipeline_ref,
                variables,
            } => self.execute_airway(pipeline_ref, variables.as_ref()).await,
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
                return self
                    .execute_workflow(&run.id, workflow_ref, None, None, false, None, None)
                    .await;
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
        extra: Option<&serde_json::Value>,
    ) -> Result<ExecutingTask, String> {
        let mut pb =
            PipelineBuilder::new(self.platform.clone()).workspace_id(self.platform.workspace_id());
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

        // `extra` is an envelope packed by `agentic-workflow` carrying
        // domain-opaque per-agent knobs. Today it carries the
        // analytics SQL-gen mode flag (`output_mode == "sql"`); the
        // builder path ignores it.
        if !is_builder_agent(agent_id)
            && let Some(extra_value) = extra
            && let Some(mode) = extra_value.get("output_mode").and_then(|v| v.as_str())
            && mode == "sql"
        {
            builder = builder.analytics_sql_mode();
        }

        // For delegation children, use the coordinator-assigned run_id
        // and skip the duplicate DB insert.
        if let Some(run_id) = existing_run_id.clone() {
            builder = builder.existing_run(run_id);
        }

        // Gate HITL when an agent runs as a delegation child
        // (existing_run_id is set → the coordinator created this
        // task). The parent workflow's SSE stream doesn't yet
        // surface child-run events, so a nested suspension leaves
        // the workflow UI looking hung. The provider differs by
        // agent type because the expected answer shape differs:
        //
        //   - Builder: `Accept` clears file-change confirmations.
        //   - Analytics: a directive string ("proceed with best
        //     interpretation") is more useful than a literal
        //     `Accept` as the answer to an `ask_user` call.
        //
        // Lift this gate once the workflow run page streams nested
        // analytics events (see the streaming-children audit).
        if existing_run_id.is_some() {
            let provider: agentic_core::human_input::HumanInputHandle =
                if is_builder_agent(agent_id) {
                    std::sync::Arc::new(agentic_core::human_input::AutoAcceptInputProvider)
                } else {
                    std::sync::Arc::new(agentic_core::human_input::NoClarificationProvider)
                };
            builder = builder.human_input(provider);
        }

        if let Some(cache) = &self.schema_cache {
            builder = builder.schema_cache(cache.clone());
        }
        if let Some(runner) = &self.builder_test_runner {
            builder = builder.test_runner(runner.clone());
        }
        if let Some(runner) = &self.builder_app_runner {
            builder = builder.app_runner(runner.clone());
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
        // Resolve agent_id with a fallback. Top-level runs land it on
        // `metadata.agent_id` (via `start_analytics`'s insert path).
        // Delegation children are inserted by `insert_child_run` with
        // `metadata = None`, but their `task_metadata.original_spec`
        // carries the full `TaskSpec::Agent` — including `agent_id`.
        // Without this fallback, resuming a workflow → analytics
        // chain would feed `""` into `start_analytics`, which then
        // calls `base_dir.join("")` (returns the workspace root, a
        // directory) and fails with `IO error: Is a directory`.
        let agent_id = run
            .metadata
            .as_ref()
            .and_then(|m| m.get("agent_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                run.task_metadata
                    .as_ref()
                    .and_then(|m| m.get("original_spec"))
                    .and_then(|s| s.get("agent_id"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();
        let model = run
            .metadata
            .as_ref()
            .and_then(|m| m.get("model"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Resume path: `existing_run_id` will be set below, so the
        // builder skips the DB insert — workspace_id is not consulted at
        // INSERT time. We still set it for trace coherence and so any
        // future cold-resume insert lands on the right row.
        let mut builder = PipelineBuilder::new(self.platform.clone())
            .workspace_id(self.platform.workspace_id())
            .question(&run.question);
        if let Some(bridges) = self.builder_bridges.clone() {
            builder = builder.with_builder_bridges(bridges);
        }

        if let Some(cache) = &self.schema_cache {
            builder = builder.schema_cache(cache.clone());
        }
        if let Some(runner) = &self.builder_test_runner {
            builder = builder.test_runner(runner.clone());
        }
        if let Some(runner) = &self.builder_app_runner {
            builder = builder.app_runner(runner.clone());
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

    /// Dispatch a `TaskSpec::Airway`. Loads `.airway.yml` from the
    /// workspace, parses it into an [`AirwayPipelineSpec`], and hands
    /// off to `AirwayWorker` which spawns the engine run and returns
    /// the runtime-shape channel pair.
    ///
    /// `variables` is captured but not yet applied — YAML templating
    /// lands in a follow-up alongside the CLI/HTTP entry points.
    async fn execute_airway(
        &self,
        pipeline_ref: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<ExecutingTask, String> {
        // Defence-in-depth: `start_airway_run` already contained the
        // ref at submit time, but re-validate at queue-claim too (the
        // queued spec is caller-influenced). `workspace_path` resolves
        // through `PlatformContext`'s `WorkspaceContext` supertrait.
        let path =
            crate::pipeline_ref::resolve_pipeline_ref(self.platform.workspace_path(), pipeline_ref)
                .map_err(|e| format!("airway: {e}"))?;
        let yaml = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| format!("airway: read pipeline_ref `{pipeline_ref}`: {e}"))?;
        // Render with the same `variables` that `start_airway_run`
        // validated against, so the worker's document matches what the
        // submitter saw.
        let mut spec = agentic_airway::AirwayPipelineSpec::from_yaml_with_vars(&yaml, variables)
            .map_err(|e| format!("airway: parse `{pipeline_ref}`: {e}"))?;

        self.resolve_airway_source_secrets(&mut spec).await?;
        self.resolve_airway_destination(&mut spec).await?;

        let worker = agentic_airway::AirwayWorker::new(Arc::new(self.db.clone()));
        Ok(worker.execute(spec))
    }

    /// Substitute a source's `*_var` credential references with values from the
    /// platform secret manager, then strip the `_var` keys so the connector
    /// factory sees only resolved literals. Each vendor source opts in
    /// explicitly to the (field, var-key) pairs it manages as secrets.
    async fn resolve_airway_source_secrets(
        &self,
        spec: &mut agentic_airway::AirwayPipelineSpec,
    ) -> Result<(), String> {
        let kind = spec.source.kind.clone();
        // (field, var-key) pairs each vendor source supports as managed secrets.
        let pairs: &[(&str, &str)] = match kind.as_str() {
            "toast" => &[
                ("client_secret", "client_secret_var"),
                ("client_id", "client_id_var"),
            ],
            // Open-Meteo commercial API key → routes the connector to the paid
            // `customer-*` endpoint (the keyless endpoint is non-commercial only).
            "weather" => &[("api_key", "api_key_var")],
            _ => return Ok(()),
        };
        let Some(obj) = spec.source.config.as_object_mut() else {
            return Ok(());
        };
        for (field, var_key) in pairs {
            let Some(var_val) = obj.get(*var_key) else {
                continue;
            };
            let var_name = var_val.as_str().ok_or_else(|| {
                format!("airway {kind}: `{var_key}` must be a string secret name")
            })?;
            let secret = self
                .platform
                .resolve_secret(var_name)
                .await
                .ok_or_else(|| {
                    format!(
                        "airway {kind}: secret `{var_name}` (referenced by `{var_key}`) \
                     could not be resolved from the secret manager"
                    )
                })?;
            obj.insert((*field).to_string(), serde_json::Value::String(secret));
            obj.remove(*var_key);
        }
        Ok(())
    }

    /// Turn a `destination: { database, dataset_name }` reference into a
    /// concrete inline connector by resolving the named `config.yml`
    /// database through the platform (secret substitution + per-subject
    /// `airhouse_managed` minting happen host-side). Inline destinations
    /// (the `memory` fixture, already-resolved specs) pass through.
    async fn resolve_airway_destination(
        &self,
        spec: &mut agentic_airway::AirwayPipelineSpec,
    ) -> Result<(), String> {
        let agentic_airway::DestinationSpec::Reference(ref_) = &spec.destination else {
            return Ok(());
        };
        let database = ref_.database.clone();
        let dataset_name = ref_.dataset_name.clone();
        let resolved = self
            .platform
            .resolve_pipeline_destination(&database)
            .await
            .ok_or_else(|| {
                format!(
                    "airway: destination `database: {database}` is not a known \
                     config.yml database with an airway-writable type \
                     (postgres or airhouse)"
                )
            })?;
        spec.destination =
            agentic_airway::DestinationSpec::Inline(agentic_airway::DestinationConfig {
                kind: resolved.kind,
                config: serde_json::json!({
                    "connection_string": resolved.connection_string,
                    "dataset_name": dataset_name,
                }),
            });
        Ok(())
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
