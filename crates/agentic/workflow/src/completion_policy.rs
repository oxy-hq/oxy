//! Workflow-aware [`CompletionPolicy`] for the agentic coordinator.
//!
//! The workflow executor stamps three flags onto a task's completion
//! metadata to coordinate the multi-step run:
//!
//! - `workflow_continue = true` — the just-completed task was an
//!   inline step (formatter, conditional, cache-hit, or the seed
//!   `Workflow` spec). The coordinator should chain immediately to
//!   the next `WorkflowDecision` task under the same task_id so the
//!   decider can advance the run.
//! - `workflow_waiting_siblings = true` — this task was one branch
//!   of a parallel fan-out and other siblings are still in flight.
//!   The eventual last-sibling completion will drive the run; this
//!   one is a no-op.
//! - `workflow_version_conflict = true` — the decider lost the
//!   optimistic-concurrency CAS on `decision_version` (a peer
//!   coordinator advanced the run between our load and commit).
//!   That peer is now driving the run; this completion is a no-op.
//!
//! Without this policy, the `agentic-runtime::coordinator` would
//! have to hard-code these flag names and the `WorkflowDecision`
//! spec variant — which it used to, and which violated the
//! "runtime is domain-agnostic" boundary.

use agentic_core::delegation::TaskSpec;
use agentic_runtime::coordinator::{
    CompletionAction, CompletionContext, CompletionPolicy, DelegationResolver,
};
use async_trait::async_trait;

/// Returns [`CompletionAction::Chain`] / [`CompletionAction::Defer`]
/// when the task's metadata carries one of the workflow flags;
/// otherwise [`CompletionAction::Finalize`].
///
/// Pass this to `Coordinator::with_completion_policy` for any
/// coordinator that may handle workflow tasks. In production every
/// coordinator gets it — workflows can be delegated from analytics
/// or builder runs as child tasks, so any coordinator could see a
/// `workflow_continue` outcome.
pub struct WorkflowCompletionPolicy;

#[async_trait]
impl CompletionPolicy for WorkflowCompletionPolicy {
    async fn on_task_done<'a>(&self, ctx: &CompletionContext<'a>) -> CompletionAction {
        let Some(meta) = ctx.metadata else {
            return CompletionAction::Finalize;
        };

        if flag(meta, "workflow_continue") {
            // Chain to the next `WorkflowDecision` under the same
            // task_id. The decider loads state from the DB, so we
            // don't carry an in-memory child answer here.
            return CompletionAction::Chain {
                spec: TaskSpec::WorkflowDecision {
                    run_id: ctx.run_id.to_string(),
                    pending_child_answer: None,
                },
            };
        }

        if flag(meta, "workflow_waiting_siblings") || flag(meta, "workflow_version_conflict") {
            return CompletionAction::Defer;
        }

        CompletionAction::Finalize
    }
}

fn flag(meta: &serde_json::Value, key: &str) -> bool {
    meta.get(key).and_then(|v| v.as_bool()) == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx<'a>(meta: &'a serde_json::Value) -> CompletionContext<'a> {
        CompletionContext {
            task_id: "t1",
            run_id: "r1",
            parent_task_id: None,
            answer: "",
            metadata: Some(meta),
        }
    }

    #[tokio::test]
    async fn finalize_when_no_metadata() {
        let policy = WorkflowCompletionPolicy;
        let ctx = CompletionContext {
            task_id: "t1",
            run_id: "r1",
            parent_task_id: None,
            answer: "",
            metadata: None,
        };
        assert!(matches!(
            policy.on_task_done(&ctx).await,
            CompletionAction::Finalize
        ));
    }

    #[tokio::test]
    async fn finalize_when_metadata_has_no_workflow_flags() {
        let policy = WorkflowCompletionPolicy;
        let meta = json!({ "unrelated": 42 });
        assert!(matches!(
            policy.on_task_done(&ctx(&meta)).await,
            CompletionAction::Finalize
        ));
    }

    #[tokio::test]
    async fn chain_on_workflow_continue() {
        let policy = WorkflowCompletionPolicy;
        let meta = json!({ "workflow_continue": true });
        match policy.on_task_done(&ctx(&meta)).await {
            CompletionAction::Chain { spec } => match spec {
                TaskSpec::WorkflowDecision {
                    run_id,
                    pending_child_answer,
                } => {
                    assert_eq!(run_id, "r1");
                    assert!(pending_child_answer.is_none());
                }
                other => panic!("expected WorkflowDecision spec, got {other:?}"),
            },
            other => panic!("expected Chain, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn defer_on_waiting_siblings() {
        let policy = WorkflowCompletionPolicy;
        let meta = json!({ "workflow_waiting_siblings": true });
        assert!(matches!(
            policy.on_task_done(&ctx(&meta)).await,
            CompletionAction::Defer
        ));
    }

    #[tokio::test]
    async fn defer_on_version_conflict() {
        let policy = WorkflowCompletionPolicy;
        let meta = json!({ "workflow_version_conflict": true });
        assert!(matches!(
            policy.on_task_done(&ctx(&meta)).await,
            CompletionAction::Defer
        ));
    }

    #[tokio::test]
    async fn flag_value_must_be_true_not_truthy() {
        let policy = WorkflowCompletionPolicy;
        // The historical inline implementation also required strict
        // `Some(true)`; a "true" string or `1` would not have
        // chained. Lock that semantic in.
        let meta = json!({ "workflow_continue": "true" });
        assert!(matches!(
            policy.on_task_done(&ctx(&meta)).await,
            CompletionAction::Finalize
        ));
    }
}

// ── Workflow delegation resolver ────────────────────────────────────────────

/// Resolves `DelegationTarget::Workflow` into the right
/// [`TaskSpec`] variant based on the shape of the `context` JSON
/// payload the workflow decider attached when suspending:
///
/// - `context.step_config` is a `{name, tasks}` sub-workflow shape
///   → inline-body [`TaskSpec::Workflow`] with
///   `workflow_ref = "__inline_workflow__"`. Each loop iteration
///   gets dispatched as a synthetic sub-workflow run so its inner
///   tasks (including agent steps) drive through the normal queue.
/// - `context.step_config` has a `type` field → [`TaskSpec::WorkflowStep`]
///   so the worker runs the single step via `run_workflow_step`.
/// - No `step_config` → vanilla [`TaskSpec::Workflow`] reading the
///   `.workflow.yml` off disk.
///
/// Used to live as a private free function in the runtime crate's
/// coordinator; moved here so the runtime stays domain-agnostic. The
/// Agent branch falls back to the trait's default impl.
pub struct WorkflowDelegationResolver;

impl DelegationResolver for WorkflowDelegationResolver {
    fn resolve_workflow(
        &self,
        workflow_ref: String,
        _request: String,
        context: serde_json::Value,
    ) -> TaskSpec {
        let step_config = context.get("step_config");
        let is_subworkflow_body = step_config
            .map(|sc| sc.get("type").is_none() && sc.get("tasks").is_some())
            .unwrap_or(false);

        if is_subworkflow_body {
            return TaskSpec::Workflow {
                workflow_ref: "__inline_workflow__".to_string(),
                variables: None,
                retry_from_run_id: None,
                cache_enabled: false,
                body: step_config.cloned(),
                initial_render_context: context.get("render_context").cloned(),
            };
        }

        if step_config.is_some() {
            return TaskSpec::WorkflowStep {
                step_config: step_config.cloned().unwrap_or_default(),
                render_context: context.get("render_context").cloned().unwrap_or_default(),
                workflow_context: context.get("workflow_context").cloned().unwrap_or_default(),
            };
        }

        TaskSpec::Workflow {
            workflow_ref,
            variables: if context.is_null() {
                None
            } else {
                Some(context)
            },
            retry_from_run_id: None,
            cache_enabled: false,
            body: None,
            initial_render_context: None,
        }
    }
}

#[cfg(test)]
mod resolver_tests {
    use super::*;
    use agentic_core::delegation::DelegationTarget;
    use serde_json::json;

    fn workflow_target() -> DelegationTarget {
        DelegationTarget::Workflow {
            workflow_ref: "workflows/example.workflow.yml".to_string(),
        }
    }

    #[test]
    fn loop_body_routes_to_inline_workflow() {
        let resolver = WorkflowDelegationResolver;
        // step_config has `tasks` but no `type` → subworkflow body shape.
        let ctx = json!({
            "step_config": { "name": "iter", "tasks": [{ "type": "execute_sql" }] },
            "render_context": { "schedules": { "value": "1" } },
        });
        match resolver.resolve(workflow_target(), "ignored".into(), ctx) {
            TaskSpec::Workflow {
                workflow_ref,
                body,
                initial_render_context,
                ..
            } => {
                assert_eq!(workflow_ref, "__inline_workflow__");
                assert!(body.is_some(), "loop body should populate `body`");
                assert!(initial_render_context.is_some());
            }
            other => panic!("expected inline-body Workflow, got {other:?}"),
        }
    }

    #[test]
    fn single_step_routes_to_workflow_step() {
        let resolver = WorkflowDelegationResolver;
        // step_config has `type` → single workflow step.
        let ctx = json!({
            "step_config": { "type": "execute_sql", "sql": "SELECT 1" },
            "render_context": {},
            "workflow_context": {},
        });
        match resolver.resolve(workflow_target(), "ignored".into(), ctx) {
            TaskSpec::WorkflowStep { step_config, .. } => {
                assert_eq!(
                    step_config.get("type").and_then(|v| v.as_str()),
                    Some("execute_sql")
                );
            }
            other => panic!("expected WorkflowStep, got {other:?}"),
        }
    }

    #[test]
    fn bare_workflow_ref_routes_to_on_disk_workflow() {
        let resolver = WorkflowDelegationResolver;
        // No step_config in context → on-disk YAML.
        let ctx = json!({ "some": "var" });
        match resolver.resolve(workflow_target(), "ignored".into(), ctx) {
            TaskSpec::Workflow {
                workflow_ref,
                body,
                variables,
                ..
            } => {
                assert_eq!(workflow_ref, "workflows/example.workflow.yml");
                assert!(body.is_none(), "on-disk workflow shouldn't carry a body");
                assert!(variables.is_some());
            }
            other => panic!("expected on-disk Workflow, got {other:?}"),
        }
    }

    #[test]
    fn agent_target_falls_through_to_default_impl() {
        // The workflow resolver only overrides `resolve_workflow`; the
        // trait's default `resolve` dispatches Agent variants to
        // `resolve_agent`, which produces a generic TaskSpec::Agent.
        let resolver = WorkflowDelegationResolver;
        let target = DelegationTarget::Agent {
            agent_id: "test_agent".to_string(),
        };
        match resolver.resolve(target, "the question".into(), json!({})) {
            TaskSpec::Agent {
                agent_id,
                question,
                extra,
            } => {
                assert_eq!(agent_id, "test_agent");
                assert_eq!(question, "the question");
                assert!(extra.is_none());
            }
            other => panic!("expected Agent spec, got {other:?}"),
        }
    }

    #[test]
    fn null_context_omits_variables_on_bare_workflow() {
        // Matches the historical inline impl: `variables: None` when
        // the context is JSON null. Lock that semantic in so changing
        // it later forces a deliberate decision.
        let resolver = WorkflowDelegationResolver;
        match resolver.resolve(workflow_target(), "ignored".into(), serde_json::Value::Null) {
            TaskSpec::Workflow {
                variables, body, ..
            } => {
                assert!(variables.is_none());
                assert!(body.is_none());
            }
            other => panic!("expected on-disk Workflow, got {other:?}"),
        }
    }
}
