//! Event bridge (deprecated).
//!
//! The `WorkflowStepOrchestrator` now emits step events directly on the
//! coordinator event channel. This module is retained as an empty placeholder
//! for backward compatibility with existing imports.

/// Placeholder — the event bridge is no longer needed.
///
/// Step events (`procedure_step_started`, `procedure_step_completed`) are now
/// emitted directly by the `WorkflowStepOrchestrator`.
pub struct WorkflowEventBridge;
