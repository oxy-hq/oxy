//! Automation-aware [`CompletionPolicy`] for the agentic coordinator.
//!
//! The automation executor stamps three flags onto a task's completion
//! metadata to coordinate the multi-step run:
//!
//! - `workflow_continue = true` — the just-completed task was an
//!   inline step (formatter, conditional, cache-hit, or the seed
//!   `Automation` spec). The coordinator should chain immediately to
//!   the next `AutomationDecision` task under the same task_id so the
//!   decider can advance the run.
//! - `workflow_waiting_siblings = true` — this task was one branch
//!   of a parallel fan-out and other siblings are still in flight.
//!   The eventual last-sibling completion will drive the run; this
//!   one is a no-op.
//! - `workflow_version_conflict = true` — the decider lost the
//!   optimistic-concurrency CAS on `decision_version` (a peer
//!   coordinator advanced the run between our load and commit).
//!   That peer is now driving the run; this completion is a no-op.
//! - `workflow_claim_lost = true` — the decision task's queue claim
//!   was handed back by graceful shutdown and re-claimed by a peer
//!   before this decision committed, so `commit_decision` rolled
//!   back. Same shape as a version conflict: the peer owns the run.
//!
//! Without this policy, the `agentic-runtime::coordinator` would
//! have to hard-code these flag names and the `AutomationDecision`
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
/// coordinator that may handle automation tasks. In production every
/// coordinator gets it — automations can be delegated from analytics
/// or builder runs as child tasks, so any coordinator could see a
/// `workflow_continue` outcome.
pub struct AutomationCompletionPolicy;

#[async_trait]
impl CompletionPolicy for AutomationCompletionPolicy {
    async fn on_task_done<'a>(&self, ctx: &CompletionContext<'a>) -> CompletionAction {
        let Some(meta) = ctx.metadata else {
            return CompletionAction::Finalize;
        };

        if flag(meta, "workflow_continue") {
            // Chain to the next `AutomationDecision` under the same
            // task_id. The decider loads state from the DB, so we
            // don't carry an in-memory child answer here.
            return CompletionAction::Chain {
                spec: TaskSpec::AutomationDecision {
                    run_id: ctx.run_id.to_string(),
                    pending_child_answer: None,
                },
            };
        }

        if flag(meta, "workflow_waiting_siblings")
            || flag(meta, "workflow_version_conflict")
            || flag(meta, "workflow_claim_lost")
        {
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
        let policy = AutomationCompletionPolicy;
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
    async fn finalize_when_metadata_has_no_automation_flags() {
        let policy = AutomationCompletionPolicy;
        let meta = json!({ "unrelated": 42 });
        assert!(matches!(
            policy.on_task_done(&ctx(&meta)).await,
            CompletionAction::Finalize
        ));
    }

    #[tokio::test]
    async fn chain_on_automation_continue() {
        let policy = AutomationCompletionPolicy;
        let meta = json!({ "workflow_continue": true });
        match policy.on_task_done(&ctx(&meta)).await {
            CompletionAction::Chain { spec } => match spec {
                TaskSpec::AutomationDecision {
                    run_id,
                    pending_child_answer,
                } => {
                    assert_eq!(run_id, "r1");
                    assert!(pending_child_answer.is_none());
                }
                other => panic!("expected AutomationDecision spec, got {other:?}"),
            },
            other => panic!("expected Chain, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn defer_on_waiting_siblings() {
        let policy = AutomationCompletionPolicy;
        let meta = json!({ "workflow_waiting_siblings": true });
        assert!(matches!(
            policy.on_task_done(&ctx(&meta)).await,
            CompletionAction::Defer
        ));
    }

    #[tokio::test]
    async fn defer_on_version_conflict() {
        let policy = AutomationCompletionPolicy;
        let meta = json!({ "workflow_version_conflict": true });
        assert!(matches!(
            policy.on_task_done(&ctx(&meta)).await,
            CompletionAction::Defer
        ));
    }

    #[tokio::test]
    async fn flag_value_must_be_true_not_truthy() {
        let policy = AutomationCompletionPolicy;
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

// ── Automation delegation resolver ────────────────────────────────────────────

/// Resolves `DelegationTarget::Automation` into the right
/// [`TaskSpec`] variant based on the shape of the `context` JSON
/// payload the automation decider attached when suspending:
///
/// - `context.step_config` is a `{name, tasks}` sub-automation shape
///   → inline-body [`TaskSpec::Automation`] with
///   `workflow_ref = "__inline_workflow__"`. Each loop iteration
///   gets dispatched as a synthetic sub-automation run so its inner
///   tasks (including agent steps) drive through the normal queue.
/// - `context.step_config` has a `type` field → [`TaskSpec::AutomationStep`]
///   so the worker runs the single step via `run_automation_step`.
/// - No `step_config` → vanilla [`TaskSpec::Automation`] reading the
///   `.automation.yml` off disk.
///
/// Used to live as a private free function in the runtime crate's
/// coordinator; moved here so the runtime stays domain-agnostic. The
/// Agent branch falls back to the trait's default impl.
pub struct AutomationDelegationResolver;

impl DelegationResolver for AutomationDelegationResolver {
    fn resolve_automation(
        &self,
        workflow_ref: String,
        _request: String,
        context: serde_json::Value,
    ) -> TaskSpec {
        // Airway steps tunnel through an `Automation` target (DelegationTarget
        // has no `Airway` variant) with the `__airway__` sentinel and the real
        // spec under `airway_spec`. Rebuild it before the sub-automation
        // routing below, which would otherwise try to load an on-disk
        // automation named "__airway__".
        if workflow_ref == "__airway__" {
            if let Some(spec) = context
                .get("airway_spec")
                .and_then(|v| serde_json::from_value::<TaskSpec>(v.clone()).ok())
            {
                return spec;
            }
        }

        let step_config = context.get("step_config");
        let is_sub_automation_body = step_config
            .map(|sc| sc.get("type").is_none() && sc.get("tasks").is_some())
            .unwrap_or(false);

        if is_sub_automation_body {
            return TaskSpec::Automation {
                workflow_ref: "__inline_workflow__".to_string(),
                variables: None,
                retry_from_run_id: None,
                cache_enabled: false,
                body: step_config.cloned(),
                initial_render_context: context.get("render_context").cloned(),
            };
        }

        if step_config.is_some() {
            return TaskSpec::AutomationStep {
                step_config: step_config.cloned().unwrap_or_default(),
                render_context: context.get("render_context").cloned().unwrap_or_default(),
                workflow_context: context.get("workflow_context").cloned().unwrap_or_default(),
            };
        }

        TaskSpec::Automation {
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

    fn automation_target() -> DelegationTarget {
        DelegationTarget::Automation {
            workflow_ref: "workflows/example.automation.yml".to_string(),
        }
    }

    #[test]
    fn loop_body_routes_to_inline_automation() {
        let resolver = AutomationDelegationResolver;
        // step_config has `tasks` but no `type` → sub-automation body shape.
        let ctx = json!({
            "step_config": { "name": "iter", "tasks": [{ "type": "execute_sql" }] },
            "render_context": { "schedules": { "value": "1" } },
        });
        match resolver.resolve(automation_target(), "ignored".into(), ctx) {
            TaskSpec::Automation {
                workflow_ref,
                body,
                initial_render_context,
                ..
            } => {
                assert_eq!(workflow_ref, "__inline_workflow__");
                assert!(body.is_some(), "loop body should populate `body`");
                assert!(initial_render_context.is_some());
            }
            other => panic!("expected inline-body Automation, got {other:?}"),
        }
    }

    #[test]
    fn single_step_routes_to_automation_step() {
        let resolver = AutomationDelegationResolver;
        // step_config has `type` → single automation step.
        let ctx = json!({
            "step_config": { "type": "execute_sql", "sql": "SELECT 1" },
            "render_context": {},
            "workflow_context": {},
        });
        match resolver.resolve(automation_target(), "ignored".into(), ctx) {
            TaskSpec::AutomationStep { step_config, .. } => {
                assert_eq!(
                    step_config.get("type").and_then(|v| v.as_str()),
                    Some("execute_sql")
                );
            }
            other => panic!("expected AutomationStep, got {other:?}"),
        }
    }

    #[test]
    fn bare_automation_ref_routes_to_on_disk_automation() {
        let resolver = AutomationDelegationResolver;
        // No step_config in context → on-disk YAML.
        let ctx = json!({ "some": "var" });
        match resolver.resolve(automation_target(), "ignored".into(), ctx) {
            TaskSpec::Automation {
                workflow_ref,
                body,
                variables,
                ..
            } => {
                assert_eq!(workflow_ref, "workflows/example.automation.yml");
                assert!(body.is_none(), "on-disk automation shouldn't carry a body");
                assert!(variables.is_some());
            }
            other => panic!("expected on-disk Automation, got {other:?}"),
        }
    }

    #[test]
    fn airway_sentinel_rebuilds_the_airway_spec() {
        // An `airway` step tunnels through an `Automation` target with the
        // `__airway__` sentinel and the serialized spec under `airway_spec`.
        // The resolver must rebuild `TaskSpec::Airway` — not try to load an
        // on-disk automation named "__airway__".
        let resolver = AutomationDelegationResolver;
        let original = TaskSpec::Airway {
            pipeline_ref: "pipelines/toast_pos.airway.yml".to_string(),
            variables: None,
            resources: vec!["orders".to_string()],
            backfill_from: None,
            backfill_to: None,
        };
        let target = DelegationTarget::Automation {
            workflow_ref: "__airway__".to_string(),
        };
        let ctx = json!({ "airway_spec": serde_json::to_value(&original).unwrap() });
        match resolver.resolve(target, "ignored".into(), ctx) {
            TaskSpec::Airway {
                pipeline_ref,
                resources,
                ..
            } => {
                assert_eq!(pipeline_ref, "pipelines/toast_pos.airway.yml");
                assert_eq!(resources, vec!["orders".to_string()]);
            }
            other => panic!("expected the rebuilt Airway spec, got {other:?}"),
        }
    }

    #[test]
    fn agent_target_falls_through_to_default_impl() {
        // The automation resolver only overrides `resolve_automation`; the
        // trait's default `resolve` dispatches Agent variants to
        // `resolve_agent`, which produces a generic TaskSpec::Agent.
        let resolver = AutomationDelegationResolver;
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
    fn null_context_omits_variables_on_bare_automation() {
        // Matches the historical inline impl: `variables: None` when
        // the context is JSON null. Lock that semantic in so changing
        // it later forces a deliberate decision.
        let resolver = AutomationDelegationResolver;
        match resolver.resolve(
            automation_target(),
            "ignored".into(),
            serde_json::Value::Null,
        ) {
            TaskSpec::Automation {
                variables, body, ..
            } => {
                assert!(variables.is_none());
                assert!(body.is_none());
            }
            other => panic!("expected on-disk Automation, got {other:?}"),
        }
    }
}
