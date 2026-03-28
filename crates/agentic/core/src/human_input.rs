//! Human-in-the-loop input trait and suspension types.
//!
//! [`HumanInputProvider`] is the platform abstraction for mid-run user input.
//! The CLI wires a `StdinInputProvider` (blocking, never suspends).
//! APIs and server modes wire a [`DeferredInputProvider`] that always returns
//! `Err(())`, causing the orchestrator to suspend and persist state.

use std::sync::Arc;

// ── HumanInputProvider ────────────────────────────────────────────────────────

/// Platform abstraction for synchronous human input during a pipeline run.
///
/// Implemented by:
/// - [`DeferredInputProvider`] — always returns `Err(())`, causing suspension.
/// - `StdinInputProvider` (in the CLI crate) — blocks on stdin, always returns `Ok`.
pub trait HumanInputProvider: Send + Sync {
    /// Request a human answer synchronously.
    ///
    /// - `Ok(answer)` — the answer is immediately available (e.g. CLI stdin).
    /// - `Err(())` — the answer is deferred; the pipeline should suspend and
    ///   return [`OrchestratorError::Suspended`] to the caller.
    ///
    /// [`OrchestratorError::Suspended`]: crate::orchestrator::OrchestratorError::Suspended
    fn request_sync(&self, prompt: &str, suggestions: &[String]) -> Result<String, ()>;
}

/// A cheaply-cloneable handle to a [`HumanInputProvider`].
pub type HumanInputHandle = Arc<dyn HumanInputProvider>;

// ── DeferredInputProvider ─────────────────────────────────────────────────────

/// Always defers input — causes the pipeline to suspend and return
/// [`OrchestratorError::Suspended`] to the caller.
///
/// Default provider for all non-CLI deployment targets (servers, APIs).
///
/// [`OrchestratorError::Suspended`]: crate::orchestrator::OrchestratorError::Suspended
pub struct DeferredInputProvider;

impl HumanInputProvider for DeferredInputProvider {
    fn request_sync(&self, _prompt: &str, _suggestions: &[String]) -> Result<String, ()> {
        Err(())
    }
}

// ── SuspendedRunData / ResumeInput ────────────────────────────────────────────

/// Minimal payload persisted when a pipeline suspends on an `ask_user` call.
///
/// Contains only the information needed to re-enter the correct pipeline state
/// from the beginning on resume.  No LLM message history is stored — the solver
/// constructs a synthetic `[user, assistant(ask_user), tool_result(answer)]`
/// exchange on resume using existing `LlmProvider` helpers.
///
/// # Stage data layout
///
/// | `from_state`     | `stage_data` content           |
/// |------------------|--------------------------------|
/// | `"clarifying"`   | `{}` (empty object)            |
/// | `"specifying"`   | serialized `D::Intent`         |
/// | `"solving"`      | serialized `D::Spec`           |
/// | `"interpreting"` | serialized `D::Result`         |
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SuspendedRunData {
    /// The pipeline stage that was suspended: `"clarifying"`, `"specifying"`,
    /// `"solving"`, or `"interpreting"`.
    pub from_state: String,
    /// The original user question that started this run.
    pub original_input: String,
    /// Trace ID for this run; filled by the orchestrator before returning
    /// [`OrchestratorError::Suspended`].
    ///
    /// [`OrchestratorError::Suspended`]: crate::orchestrator::OrchestratorError::Suspended
    pub trace_id: String,
    /// Domain-specific serialized prior-stage output needed to re-enter the state.
    pub stage_data: serde_json::Value,
    /// The question the LLM posed to the user via `ask_user`.
    pub question: String,
    /// Optional suggestions provided by the LLM alongside the question.
    pub suggestions: Vec<String>,
}

/// Input to [`DomainSolver::set_resume_data`] — passed by
/// [`Orchestrator::resume`] before re-entering the pipeline.
///
/// [`DomainSolver::set_resume_data`]: crate::solver::DomainSolver::set_resume_data
/// [`Orchestrator::resume`]: crate::orchestrator::Orchestrator::resume
pub struct ResumeInput {
    /// The persisted suspension data.
    pub data: SuspendedRunData,
    /// The user's answer to the `ask_user` question.
    pub answer: String,
}
