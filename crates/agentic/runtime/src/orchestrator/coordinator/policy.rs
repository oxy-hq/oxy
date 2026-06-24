//! Pluggable post-`Done` policy for the coordinator.
//!
//! `Coordinator::handle_done` used to inline three pieces of
//! agentic-automation knowledge:
//!
//! - `workflow_continue = true` metadata → chain a fresh
//!   `AutomationDecision` task under the same task_id, skipping the
//!   normal terminal finalisation.
//! - `workflow_waiting_siblings = true` or
//!   `workflow_version_conflict = true` → silently return; another
//!   path (sibling completion, peer worker's CAS) drives the run.
//! - anything else → propagate Done to the parent (or set root done).
//!
//! That hard-coded knowledge is a layering violation: the runtime
//! crate is supposed to be domain-agnostic, but `TaskSpec::AutomationDecision`
//! is an automation-domain enum variant. This trait moves the
//! decision-making out of the coordinator. The runtime ships a
//! [`DefaultCompletionPolicy`] that always finalises (correct for
//! any domain without chain semantics), and domains plug in their
//! own — see `agentic_automation::AutomationCompletionPolicy`.
//!
//! The trait is intentionally `async` so future policies can do
//! DB lookups (e.g. "is this loop already at max iterations") without
//! API churn. Sync impls pay nothing for it.

use async_trait::async_trait;
use serde_json::Value;

use agentic_core::delegation::{DelegationTarget, TaskSpec};

/// Context handed to a [`CompletionPolicy`] for each `Done` outcome.
///
/// Borrowed views only — the policy never owns task-map state. If a
/// future policy needs more info, add fields here rather than
/// exposing the coordinator's internal `TaskNode` map.
pub struct CompletionContext<'a> {
    pub task_id: &'a str,
    pub run_id: &'a str,
    pub parent_task_id: Option<&'a str>,
    pub answer: &'a str,
    pub metadata: Option<&'a Value>,
}

/// What the coordinator should do with a `Done` outcome.
#[derive(Debug)]
pub enum CompletionAction {
    /// Treat as a normal terminal Done: propagate to parent if any,
    /// otherwise set the root run `done`. This is the right default
    /// for any task whose completion has no domain-specific follow-up.
    Finalize,
    /// Suppress finalisation. Another path will drive the run
    /// forward — for the automation domain this means "parallel
    /// siblings still in flight" or "another worker won the
    /// optimistic-concurrency CAS." The coordinator does nothing
    /// further with this outcome.
    Defer,
    /// Don't finalise this task; instead, enqueue a follow-up
    /// assignment that continues the run. Used by the automation
    /// domain to chain from a completed step into the next
    /// `agentic_automation::AutomationDecision`. The coordinator re-uses the existing
    /// task node (its status stays `Running`) and assigns the new
    /// spec under the same task_id + run_id + no parent.
    Chain { spec: TaskSpec },
}

/// Decides what to do when a task reports `Done`.
///
/// Implementations must be `Send + Sync` because the coordinator
/// holds them behind an `Arc` and calls them from a single async
/// task — but in a multi-coordinator process every coordinator
/// shares the same policy instance.
#[async_trait]
pub trait CompletionPolicy: Send + Sync {
    async fn on_task_done<'a>(&self, ctx: &CompletionContext<'a>) -> CompletionAction;
}

/// Always returns [`CompletionAction::Finalize`]. Right behavior for
/// any domain that doesn't chain follow-ups or defer terminals.
///
/// Used as the [`crate::orchestrator::coordinator::Coordinator`] default so the
/// runtime tests + non-automation callers work without configuring a
/// policy. Production wires in `agentic_automation::AutomationCompletionPolicy`.
pub struct DefaultCompletionPolicy;

#[async_trait]
impl CompletionPolicy for DefaultCompletionPolicy {
    async fn on_task_done<'a>(&self, _ctx: &CompletionContext<'a>) -> CompletionAction {
        CompletionAction::Finalize
    }
}

// ── Delegation resolver ─────────────────────────────────────────────────────

/// Converts a wire-level `(DelegationTarget, request, context)` triple
/// arriving from a worker's `Suspended` outcome into a concrete
/// [`TaskSpec`] that the coordinator can re-assign through the queue.
///
/// Why a trait, not a free function: the automation domain's
/// [`DelegationTarget::Automation`] mapping isn't a simple pass-through.
/// A single `Automation` target can mean three different things
/// depending on the shape of `context`:
///
/// - context.step_config is a `{name, tasks}` sub-automation body →
///   inline-body [`TaskSpec::Automation`] (loop iterations).
/// - context.step_config has a `type` field → [`TaskSpec::AutomationStep`].
/// - no step_config → real on-disk [`TaskSpec::Automation`].
///
/// That routing is automation-domain knowledge, so it lives in
/// `agentic_automation::AutomationDelegationResolver`. The default impl
/// here handles `Agent` generically and falls back to a basic
/// `TaskSpec::Automation` mapping that doesn't do step/body inspection
/// — fine for the runtime tests and for any caller that never uses
/// the automation domain.
///
/// The trait is sync (unlike [`CompletionPolicy`]) because the
/// translation is pure: it inspects already-deserialised JSON and
/// returns a value. If we ever need DB lookups here, change to
/// async + `BoxFuture`.
pub trait DelegationResolver: Send + Sync {
    /// Dispatch by target variant. Override the variant-specific
    /// methods below rather than this one in most cases; this default
    /// just routes to them.
    fn resolve(&self, target: DelegationTarget, request: String, context: Value) -> TaskSpec {
        match target {
            DelegationTarget::Agent { agent_id } => self.resolve_agent(agent_id, request, context),
            DelegationTarget::Automation { workflow_ref } => {
                self.resolve_automation(workflow_ref, request, context)
            }
        }
    }

    /// Agent delegation has a uniform shape across domains: the
    /// `request` string becomes the agent's `question`. Override only
    /// if a domain wants to carry context-derived fields into the
    /// `TaskSpec::Agent`.
    fn resolve_agent(&self, agent_id: String, request: String, context: Value) -> TaskSpec {
        // Pluck `extra` out of context so downstream resolvers (e.g.
        // the automation SQL-gen path) can pass typed payloads through
        // without changing this trait method's signature.
        let extra = context.get("extra").filter(|v| !v.is_null()).cloned();
        TaskSpec::Agent {
            agent_id,
            question: request,
            extra,
        }
    }

    /// Default routes any `Automation` target to a vanilla
    /// `TaskSpec::Automation` reading off disk, treating `context` as
    /// `variables`. Sufficient for "delegate to a real .automation.yml"
    /// without the loop-iteration or single-step distinctions —
    /// `agentic_automation::AutomationDelegationResolver` overrides this
    /// to add those.
    fn resolve_automation(
        &self,
        workflow_ref: String,
        _request: String,
        context: Value,
    ) -> TaskSpec {
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

/// No-override impl. See [`DelegationResolver`] for the default
/// behaviour the trait provides.
pub struct DefaultDelegationResolver;

impl DelegationResolver for DefaultDelegationResolver {}
