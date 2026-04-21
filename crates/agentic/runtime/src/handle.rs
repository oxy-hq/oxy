//! Domain-agnostic pipeline handle and outcome types.

use agentic_core::delegation::SuspendReason;
use agentic_core::human_input::SuspendedRunData;
use agentic_core::{DomainEvents, Event};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Handle to a running pipeline, returned by a domain's `start_pipeline()`.
///
/// Transport-agnostic: works with HTTP SSE, gRPC streaming, CLI, or tests.
///
/// Note: HITL answers are delivered via [`crate::state::RuntimeState::answer_txs`]
/// in the coordinator architecture, not through this handle.
pub struct PipelineHandle<Ev: DomainEvents = ()> {
    /// Receive domain + core events (for persistence / streaming).
    pub events: mpsc::Receiver<Event<Ev>>,
    /// Receive pipeline outcomes. Intermediate `Suspended` outcomes are
    /// followed by a terminal `Done`/`Failed`/`Cancelled`.
    pub outcomes: mpsc::Receiver<PipelineOutcome>,
    /// Cancel the pipeline.
    pub cancel: CancellationToken,
    /// Await pipeline task completion.
    pub join: JoinHandle<()>,
}

/// Outcome of a pipeline execution step.
///
/// Domain-agnostic: the `Done` variant carries the answer as a plain string
/// with optional JSON metadata for domain-specific data (e.g. `spec_hint`).
pub enum PipelineOutcome {
    /// Pipeline completed successfully.
    Done {
        /// User-facing answer text.
        answer: String,
        /// Domain-specific metadata (e.g. analytics stores `spec_hint` here).
        metadata: Option<Value>,
    },
    /// Pipeline suspended — either for human input or agent delegation.
    Suspended {
        reason: SuspendReason,
        resume_data: SuspendedRunData,
        trace_id: String,
    },
    /// Pipeline failed with an error message.
    Failed(String),
    /// Pipeline was cancelled by the caller.
    Cancelled,
}
