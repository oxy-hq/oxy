//! Error-to-state routing for the `Diagnosing` pipeline stage.
//!
//! [`diagnose_impl`] is the single source of truth for the FSM back-edge
//! routing table.  It is called from `DomainSolver::diagnose` in `mod.rs`.

use agentic_core::{back_target::BackTarget, orchestrator::RunContext, state::ProblemState};

use crate::{AnalyticsDomain, AnalyticsError, AnalyticsIntent};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the intent to use for recovery.
///
/// Prefers the intent carried in the back-target (Clarify / Specify variants).
/// Falls back to `ctx.intent` (populated by the Clarifying stage) for back-targets
/// that carry a spec or result instead of an intent (Solve, Execute, Interpret).
/// Returns `None` only when neither source has an intent.
///
/// `Suspend` is handled by the orchestrator before `diagnose` is called, so
/// it is explicitly `unreachable!`.
fn intent_for_recovery(
    back: &BackTarget<AnalyticsDomain>,
    ctx: &RunContext<AnalyticsDomain>,
) -> Option<AnalyticsIntent> {
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

/// Route a domain error to the appropriate recovery [`ProblemState`].
///
/// # Routing table
///
/// | Error | Back carries | Recovery |
/// |---|---|---|
/// | `NeedsUserInput` | any | **Fatal** |
/// | `AmbiguousColumn` | Clarify / Specify / Solve | `Clarifying` |
/// | `AmbiguousColumn` | Execute / Interpret | **Fatal** |
/// | `UnresolvedMetric` | Clarify / Specify / Solve | `Specifying` |
/// | `UnresolvedMetric` | Execute / Interpret | **Fatal** |
/// | `UnresolvedJoin` | Clarify / Specify / Solve | `Specifying` |
/// | `UnresolvedJoin` | Execute / Interpret | **Fatal** |
/// | `SyntaxError` | Solve | `Solving` |
/// | `SyntaxError` | Clarify / Specify | `Specifying` |
/// | `SyntaxError` | Execute / Interpret | **Fatal** (fallback — custom handlers convert first) |
/// | `EmptyResults` | Clarify / Specify / Solve | `Specifying` |
/// | `EmptyResults` | Execute / Interpret | **Fatal** |
/// | `ShapeMismatch` | Solve | `Solving` |
/// | `ShapeMismatch` | Execute | **Fatal** (no spec available) |
/// | `ShapeMismatch` | Clarify / Specify | `Specifying` |
/// | `ValueAnomaly` | Interpret | `Interpreting` |
/// | `ValueAnomaly` | other | **Fatal** |
/// | `InvalidChartConfig` | Interpret | `Interpreting` (retry with error context) |
/// | `InvalidChartConfig` | other | **Fatal** |
pub(super) async fn diagnose_impl(
    error: AnalyticsError,
    back: BackTarget<AnalyticsDomain>,
    ctx: &RunContext<AnalyticsDomain>,
) -> Result<ProblemState<AnalyticsDomain>, AnalyticsError> {
    match &error {
        // ── Fatal: requires out-of-band user response ─────────────────
        AnalyticsError::NeedsUserInput { .. } => Err(error),

        // ── AmbiguousColumn → ask the user to pick ────────────────────
        AnalyticsError::AmbiguousColumn { .. } => match intent_for_recovery(&back, ctx) {
            Some(intent) => Ok(ProblemState::Clarifying(intent)),
            None => Err(error),
        },

        // ── UnresolvedMetric → re-specify with different mapping ───────
        AnalyticsError::UnresolvedMetric { .. } => match intent_for_recovery(&back, ctx) {
            Some(intent) => Ok(ProblemState::Specifying(intent)),
            None => Err(error),
        },

        // ── UnresolvedJoin → re-specify with corrected join path ───────
        AnalyticsError::UnresolvedJoin { .. } => match intent_for_recovery(&back, ctx) {
            Some(intent) => Ok(ProblemState::Specifying(intent)),
            None => Err(error),
        },

        // ── SyntaxError → regenerate SQL or re-specify ────────────────
        // NOTE: BackTarget::Execute should never reach here in normal
        // operation — the custom executing handler in build_analytics_handlers
        // and the fan-out path in the specifying handler both convert
        // BackTarget::Execute into BackTarget::Solve / BackTarget::Specify
        // before emitting ProblemState::Diagnosing.  The fatal arm below is
        // a last-resort fallback for any code path that bypasses those handlers.
        AnalyticsError::SyntaxError { .. } => match back {
            BackTarget::Solve(spec, _) => Ok(ProblemState::Solving(spec)),
            BackTarget::Clarify(i, _) | BackTarget::Specify(i, _) => {
                Ok(ProblemState::Specifying(i))
            }
            BackTarget::Execute(_, _) | BackTarget::Interpret(_, _) => Err(error),
            BackTarget::Suspend { .. } => unreachable!("BackTarget::Suspend reached diagnose"),
        },

        // ── EmptyResults → widen constraints in Specify ───────────────
        AnalyticsError::EmptyResults { .. } => match intent_for_recovery(&back, ctx) {
            Some(intent) => Ok(ProblemState::Specifying(intent)),
            None => Err(error),
        },

        // ── ShapeMismatch → restructure query or re-specify ───────────
        AnalyticsError::ShapeMismatch { .. } => match back {
            BackTarget::Solve(spec, _) => Ok(ProblemState::Solving(spec)),
            // No spec available at Execute stage → fatal.
            BackTarget::Execute(_, _) => Err(error),
            BackTarget::Clarify(i, _) | BackTarget::Specify(i, _) => {
                Ok(ProblemState::Specifying(i))
            }
            BackTarget::Interpret(_, _) => Err(error),
            BackTarget::Suspend { .. } => unreachable!("BackTarget::Suspend reached diagnose"),
        },

        // ── ValueAnomaly → pass through to Interpret (best-effort) ────
        AnalyticsError::ValueAnomaly { .. } => match back {
            BackTarget::Interpret(result, _) => Ok(ProblemState::Interpreting(result)),
            _ => Err(error),
        },

        // ── InvalidChartConfig → retry Interpret so the LLM can fix columns ──
        AnalyticsError::InvalidChartConfig { .. } => match back {
            BackTarget::Interpret(result, _) => Ok(ProblemState::Interpreting(result)),
            _ => Err(error),
        },

        // ── VendorError → fatal: vendor engine failed ─────────────────────
        AnalyticsError::VendorError { .. } => Err(error),
    }
}
