use crate::domain::Domain;
use crate::events::HumanInputQuestion;

/// Context carried on back-edges so retried stages know what went wrong.
///
/// Populated by the solver on failure and consumed by prompt builders on
/// the next attempt.  Fields are intentionally optional — per DESIGN.md,
/// anchoring on previous failures can harm generation quality.
#[derive(Debug, Clone, Default)]
pub struct RetryContext {
    /// Error messages from the failed attempt.
    pub errors: Vec<String>,
    /// How many times this target state has been entered so far.
    pub attempt: u32,
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
    /// Produce the next RetryContext by incrementing the attempt counter and
    /// appending a new error message.
    pub fn advance(self, error: String) -> Self {
        Self {
            attempt: self.attempt + 1,
            errors: {
                let mut e = self.errors;
                e.push(error);
                e
            },
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

    /// The LLM invoked `ask_user` with a [`DeferredInputProvider`] — the
    /// pipeline must suspend and present the questions to the user in the next
    /// turn.
    ///
    /// Lightweight: carries only what is needed for the
    /// [`AwaitingHumanInput`] event.  The solver stores the full
    /// [`SuspendedRunData`] internally via
    /// [`DomainSolver::store_suspension_data`].
    ///
    /// [`DeferredInputProvider`]: crate::human_input::DeferredInputProvider
    /// [`AwaitingHumanInput`]: crate::events::CoreEvent::AwaitingHumanInput
    /// [`SuspendedRunData`]: crate::human_input::SuspendedRunData
    /// [`DomainSolver::store_suspension_data`]: crate::solver::DomainSolver::store_suspension_data
    Suspend {
        /// One or more questions to present to the user. Triage may produce
        /// multiple independent ambiguity questions; tool-loop `ask_user`
        /// always produces exactly one.
        questions: Vec<HumanInputQuestion>,
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
                unreachable!("retry_ctx() called on BackTarget::Suspend — orchestrator must short-circuit before this point")
            }
        }
    }
}
