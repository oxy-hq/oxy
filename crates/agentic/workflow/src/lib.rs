pub mod completion_policy;
pub mod config;
pub mod error;
pub mod event_bridge;
pub mod export;
pub mod extension;
pub mod hash;
pub(crate) mod render;
pub mod resolve;
pub mod runner;
pub mod semantic;
pub mod semantic_bridge;
pub mod step_decider;
pub mod step_executor;
pub mod step_hash;
pub mod step_orchestrator;
pub mod variables;
pub mod workspace;

pub use completion_policy::{WorkflowCompletionPolicy, WorkflowDelegationResolver};
pub use config::{TaskType, WorkflowConfig};
pub use error::WorkflowError;
pub use event_bridge::WorkflowEventBridge;
pub use extension::WorkflowMigrator;
pub use resolve::{build_subrun_steps, resolve_subworkflows};
pub use runner::OxyProcedureRunner;
pub use step_decider::{WorkflowDecider, WorkflowDecision};
pub use step_executor::{extract_workflow_steps, run_workflow_step};
pub use step_orchestrator::WorkflowStepOrchestrator;
pub use workspace::WorkspaceContext;

/// `source_type` to register this domain under in the runtime event registry.
/// Used by the SSE layer to look up the right processor for a run's events.
pub const SOURCE_TYPE: &str = "workflow";

/// Build a [`DomainHandler`] for registering workflow events with the
/// runtime's [`EventRegistry`].
///
/// Workflow events (`subrun_started`, `subrun_step_started`,
/// `subrun_step_completed`, `subrun_step_cache_hit`,
/// `subrun_completed`) are emitted directly as `(event_type, payload)`
/// JSON pairs by the decider — no transformation needed. The processor is
/// a passthrough that preserves both fields verbatim.
pub fn event_handler() -> agentic_runtime::event_registry::DomainHandler {
    use agentic_runtime::event_registry::{DomainHandler, RowProcessor};
    use std::sync::Arc;

    let processor: RowProcessor =
        Arc::new(|event_type, payload| Some(vec![(event_type.to_string(), payload.clone())]));

    DomainHandler {
        processor,
        // Workflows have no LLM tool/state summaries — the diagram is the UI.
        summary_fn: Arc::new(|_| None),
        tool_summary_fn: Arc::new(|_, _| None),
        // Workflow events are independent — no StepEnd metadata accumulation.
        should_accumulate: Some(Arc::new(|_| false)),
    }
}
