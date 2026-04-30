use crate::delegation::SuspendReason;
use crate::domain::Domain;

/// Context carried on back-edges so retried stages know what went wrong.
///
/// Populated by the solver on failure and consumed by prompt builders on
/// the next attempt.  Fields are intentionally optional — per DESIGN.md,
/// anchoring on previous failures can harm generation quality.
#[derive(Debug, Clone, Default)]
pub struct RetryContext {
    /// Error messages from the failed attempt.
    pub errors: Vec<String>,
    /// How many times this target state has been entered due to transient errors.
    pub attempt: u32,
    /// How many times this target state has been retried due to rate-limit (429)
    /// responses.  Kept separate from `attempt` so the two budgets don't
    /// interfere: burning rate-limit retries shouldn't exhaust the transient-error
    /// budget and vice-versa.
    pub rate_limit_attempt: u32,
    /// The previous output that failed (e.g. the SQL that had a syntax error).
    ///
    /// Only populated after the second failed attempt (`attempt >= 2`) to
    /// avoid anchoring on the very first failure.
    pub previous_output: Option<String>,
}

/// Identifies which pipeline stage to return to, and carries the data needed
/// to reconstruct that stage's state.
///
/// Back-edges are produced by solver methods on failure and stored inside
/// [`ProblemState::Diagnosing`].  The `diagnose` method then converts them
/// into a concrete [`ProblemState`] variant for the next iteration.
///
/// Each variant also carries a [`RetryContext`] so that retried stages have
/// information about *why* they are being retried.  Default-construct
/// `RetryContext` at every call site that does not have failure information
/// yet.
///
/// # The `HasIntent` connection
///
/// `BackTarget::Solve(spec, _)` is the key case where `D::Spec: HasIntent<D>`
/// becomes useful: to construct a `BackTarget::Specify` from inside `solve`,
/// the solver calls `spec.intent().clone()` — it can recover the intent even
/// though the intent was not stored anywhere else in the Solve stage.
///
/// [`ProblemState::Diagnosing`]: crate::state::ProblemState::Diagnosing
/// [`ProblemState`]: crate::state::ProblemState
impl RetryContext {
    /// Produce the next RetryContext by incrementing the transient-error attempt
    /// counter and appending a new error message.
    pub fn advance(self, error: String) -> Self {
        Self {
            attempt: self.attempt + 1,
            errors: {
                let mut e = self.errors;
                e.push(error);
                e
            },
            rate_limit_attempt: self.rate_limit_attempt,
            previous_output: self.previous_output,
        }
    }

    /// Produce the next RetryContext by incrementing only the rate-limit attempt
    /// counter.  The transient-error `attempt` field is intentionally left
    /// unchanged so the two budgets remain independent.
    pub fn advance_rate_limit(self, error: String) -> Self {
        Self {
            rate_limit_attempt: self.rate_limit_attempt + 1,
            errors: {
                let mut e = self.errors;
                e.push(error);
                e
            },
            attempt: self.attempt,
            previous_output: self.previous_output,
        }
    }
}

pub enum BackTarget<D: Domain> {
    /// Return to the **Clarify** stage with the given (possibly revised) intent.
    Clarify(D::Intent, RetryContext),

    /// Return to the **Specify** stage with the given intent.
    Specify(D::Intent, RetryContext),

    /// Return to the **Solve** stage with the given spec.
    ///
    /// The originating intent is recoverable via [`HasIntent::intent`].
    ///
    /// [`HasIntent::intent`]: crate::domain::HasIntent::intent
    Solve(D::Spec, RetryContext),

    /// Return to the **Execute** stage with the given solution.
    Execute(D::Solution, RetryContext),

    /// Return to the **Interpret** stage with the given result.
    Interpret(D::Result, RetryContext),

    /// The pipeline must suspend — either to ask the user a question or to
    /// delegate work to another agent/workflow.
    ///
    /// The [`SuspendReason`] tells the coordinator how to fulfil the
    /// suspension.  The solver stores the full [`SuspendedRunData`]
    /// internally via [`DomainSolver::store_suspension_data`] before
    /// returning this variant.
    ///
    /// [`SuspendReason`]: crate::delegation::SuspendReason
    /// [`SuspendedRunData`]: crate::human_input::SuspendedRunData
    /// [`DomainSolver::store_suspension_data`]: crate::solver::DomainSolver::store_suspension_data
    Suspend {
        /// Why the pipeline is suspending.
        reason: SuspendReason,
    },
}

impl<D: Domain> BackTarget<D> {
    /// Return a reference to the [`RetryContext`] carried by this back-edge.
    ///
    /// # Panics
    ///
    /// Panics if called on [`BackTarget::Suspend`] — the orchestrator
    /// short-circuits before calling this method for suspension back-edges.
    pub fn retry_ctx(&self) -> &RetryContext {
        match self {
            BackTarget::Clarify(_, ctx)
            | BackTarget::Specify(_, ctx)
            | BackTarget::Solve(_, ctx)
            | BackTarget::Execute(_, ctx)
            | BackTarget::Interpret(_, ctx) => ctx,
            BackTarget::Suspend { .. } => {
                unreachable!(
                    "retry_ctx() called on BackTarget::Suspend — orchestrator must short-circuit before this point"
                )
            }
        }
    }
}
