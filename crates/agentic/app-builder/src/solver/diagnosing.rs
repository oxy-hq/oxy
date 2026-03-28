//! Error-to-state routing for the `Diagnosing` pipeline stage.

use agentic_core::{back_target::BackTarget, orchestrator::RunContext, state::ProblemState};

use crate::types::{AppBuilderDomain, AppBuilderError, AppIntent};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the intent from the back-target or run context for recovery.
fn intent_for_recovery(
    back: &BackTarget<AppBuilderDomain>,
    ctx: &RunContext<AppBuilderDomain>,
) -> Option<AppIntent> {
    match back {
        BackTarget::Clarify(i, _) | BackTarget::Specify(i, _) => Some(i.clone()),
        BackTarget::Solve(_, _) | BackTarget::Execute(_, _) | BackTarget::Interpret(_, _) => {
            ctx.intent.clone()
        }
        BackTarget::Suspend { .. } => unreachable!("BackTarget::Suspend reached diagnose"),
    }
}

// ---------------------------------------------------------------------------
// Routing table
// ---------------------------------------------------------------------------

pub(super) async fn diagnose_impl(
    error: AppBuilderError,
    back: BackTarget<AppBuilderDomain>,
    ctx: &RunContext<AppBuilderDomain>,
) -> Result<ProblemState<AppBuilderDomain>, AppBuilderError> {
    match &error {
        // NeedsUserInput is always fatal.
        AppBuilderError::NeedsUserInput { .. } => Err(error),

        // UnresolvedTable → re-clarify to pick the right table.
        AppBuilderError::UnresolvedTable { .. } => match intent_for_recovery(&back, ctx) {
            Some(intent) => Ok(ProblemState::Clarifying(intent)),
            None => Err(error),
        },

        // UnresolvedColumn → re-specify with corrected column list.
        AppBuilderError::UnresolvedColumn { .. } => match intent_for_recovery(&back, ctx) {
            Some(intent) => Ok(ProblemState::Specifying(intent)),
            None => Err(error),
        },

        // SyntaxError → regenerate SQL in Solving.
        AppBuilderError::SyntaxError { .. } => match back {
            BackTarget::Solve(spec, _) => Ok(ProblemState::Solving(spec)),
            BackTarget::Execute(_, _) => {
                // Execution-time syntax error: back to Solving if we have a spec.
                match ctx.spec.clone() {
                    Some(spec) => Ok(ProblemState::Solving(spec)),
                    None => Err(error),
                }
            }
            BackTarget::Clarify(i, _) | BackTarget::Specify(i, _) => {
                Ok(ProblemState::Specifying(i))
            }
            BackTarget::Interpret(_, _) => Err(error),
            BackTarget::Suspend { .. } => unreachable!("BackTarget::Suspend reached diagnose"),
        },

        // EmptyResults → widen constraints in Specify.
        AppBuilderError::EmptyResults { .. } => match intent_for_recovery(&back, ctx) {
            Some(intent) => Ok(ProblemState::Specifying(intent)),
            None => Err(error),
        },

        // ShapeMismatch → re-solve or re-specify.
        AppBuilderError::ShapeMismatch { .. } => match back {
            BackTarget::Solve(spec, _) => Ok(ProblemState::Solving(spec)),
            BackTarget::Execute(_, _) => Err(error),
            BackTarget::Clarify(i, _) | BackTarget::Specify(i, _) => {
                Ok(ProblemState::Specifying(i))
            }
            BackTarget::Interpret(_, _) => Err(error),
            BackTarget::Suspend { .. } => unreachable!("BackTarget::Suspend reached diagnose"),
        },

        // InvalidSpec → re-specify with corrected structure.
        AppBuilderError::InvalidSpec { .. } => match intent_for_recovery(&back, ctx) {
            Some(intent) => Ok(ProblemState::Specifying(intent)),
            None => Err(error),
        },

        // InvalidChartConfig → retry Interpreting.
        AppBuilderError::InvalidChartConfig { .. } => match back {
            BackTarget::Interpret(result, _) => Ok(ProblemState::Interpreting(result)),
            _ => Err(error),
        },
    }
}
