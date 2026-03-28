use crate::{back_target::BackTarget, domain::Domain};

/// Every possible state of a problem as it flows through the pipeline.
///
/// The orchestrator holds exactly one `ProblemState<D>` at a time and
/// advances it on each iteration of its run-loop.
pub enum ProblemState<D: Domain> {
    /// The intent is being clarified or refined before any spec is produced.
    Clarifying(D::Intent),

    /// A structured [`Spec`] is being derived from the (clarified) intent.
    ///
    /// [`Spec`]: Domain::Spec
    Specifying(D::Intent),

    /// A concrete [`Solution`] is being planned from the spec.
    ///
    /// [`Solution`]: Domain::Solution
    Solving(D::Spec),

    /// The solution is being executed in the environment to produce a result.
    Executing(D::Solution),

    /// Raw [`Result`] output is being interpreted into a user-facing answer.
    ///
    /// [`Result`]: Domain::Result
    Interpreting(D::Result),

    /// An error occurred; the solver is diagnosing it and will choose a
    /// [`BackTarget`] to recover.
    Diagnosing {
        error: D::Error,
        back: BackTarget<D>,
    },

    /// The pipeline completed successfully.
    Done(D::Answer),
}
