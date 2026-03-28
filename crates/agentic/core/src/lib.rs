//! # agentic-core
//!
//! Generic framework for agentic problem-solving pipelines.
//!
//! ## Pipeline stages
//!
//! ```text
//!  Intent
//!    │
//!    ▼
//! Clarifying ──► Specifying ──► Solving ──► Executing ──► Interpreting ──► Done
//!    ▲               ▲             ▲             ▲               ▲
//!    └───────────────┴─────────────┴─────────────┴───────────────┘
//!                        Diagnosing (back-edges)
//! ```
//!
//! Implement [`Domain`] to declare your associated types, [`HasIntent`] on
//! your `Spec` type to enable intent recovery on back-edges, and
//! [`DomainSolver`] to provide the async logic for each stage.  Then wrap
//! everything in an [`Orchestrator`] and call [`Orchestrator::run`].

pub mod back_target;
pub mod domain;
pub mod events;
pub mod human_input;
pub mod orchestrator;
pub mod result;
pub mod solver;
pub mod state;
pub mod tools;
pub mod ui_stream;

#[cfg(feature = "storage")]
pub mod app_storage;
#[cfg(feature = "storage")]
pub mod storage;

pub use back_target::{BackTarget, RetryContext};
pub use domain::Domain;
pub use events::{CoreEvent, DomainEvents, Event, EventStream, HumanInputQuestion, Outcome};
pub use human_input::{
    DeferredInputProvider, HumanInputHandle, HumanInputProvider, ResumeInput, SuspendedRunData,
};
pub use orchestrator::{
    build_default_handlers, child_trace_id, next_trace_id, run_fanout, CompletedTurn, Orchestrator,
    OrchestratorError, PipelineOutput, RunContext, SessionMemory, StateHandler, TransitionResult,
};
pub use result::{CellValue, QueryResult, QueryRow};
pub use solver::{DomainSolver, FanoutWorker};
pub use state::ProblemState;
pub use tools::{ToolDef, ToolError};
pub use ui_stream::{UiBlock, UiTransformState};

#[cfg(feature = "storage")]
pub use app_storage::{
    truncate_artifact_content, Artifact, PersistedTurn, PreferenceStore, QueryLog, QueryLogEntry,
    SessionSummary, StorageError, StorageHandle, SuspendedPipeline, SuspendedPipelineStore,
    TurnStore,
};
#[cfg(feature = "storage")]
pub use storage::{InMemoryStorage, JsonFileStorage};
