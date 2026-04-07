//! Human-in-the-loop (HITL) resume path helpers.
//!
//! Contains the `ask_user` tool definition used in both Clarifying and
//! Specifying states, its executor, and the suspend/resume state routing
//! function used by the [`DomainSolver`] trait impl.
//!
//! # Note on `ask_user` interception
//!
//! `ask_user_tool_def` is listed in `tools_for_state` for the "clarifying"
//! and "specifying" states so the LLM can see and call it.  However it is
//! **not** dispatched through `DomainSolver::execute_tool` — instead the LLM
//! tool loop inside `clarify_impl` / `specify_impl` intercepts `ask_user`
//! calls directly before the tool dispatcher is reached.  `execute_tool`
//! therefore never sees `ask_user`; the listing in `tools_for_state` is purely
//! for the LLM's function-call schema.

use agentic_core::{human_input::SuspendedRunData, state::ProblemState, tools::ToolDef};

use crate::types::QuestionType;
use crate::{AnalyticsDomain, AnalyticsIntent, AnalyticsResult, QuerySpec};
use agentic_core::result::QueryResult;

// ---------------------------------------------------------------------------
// ask_user tool — thin wrappers over agentic_core shared implementation
// ---------------------------------------------------------------------------

/// Tool definition for `ask_user`, with OpenAI `additionalProperties: false`
/// injected for strict-mode compatibility.
pub(super) fn ask_user_tool_def() -> ToolDef {
    use crate::llm::inject_additional_properties_false;
    let mut def = agentic_core::tools::ask_user_tool_def();
    inject_additional_properties_false(&mut def.parameters);
    def
}

/// Re-export the shared `handle_ask_user` from core.
pub(super) use agentic_core::tools::handle_ask_user;

// ---------------------------------------------------------------------------
// Resume routing
// ---------------------------------------------------------------------------

/// Reconstruct the [`ProblemState`] to re-enter when resuming a suspended run.
///
/// Called from `DomainSolver::problem_state_from_resume` in the trait impl.
///
/// # Fallback on unknown `from_state`
///
/// Unknown or corrupt `from_state` values log a warning and fall back to
/// `Clarifying` (safest re-entry point — triage will be skipped because
/// `resume_data` is set).  This avoids a panic on stale suspension data.
pub(super) fn problem_state_from_resume(data: &SuspendedRunData) -> ProblemState<AnalyticsDomain> {
    match data.from_state.as_str() {
        "clarifying" => {
            // Re-enter clarifying with a minimal intent built from the
            // original question; triage will be skipped because resume_data
            // is set.
            ProblemState::Clarifying(AnalyticsIntent {
                raw_question: data.original_input.clone(),
                summary: String::new(),
                question_type: QuestionType::SingleValue,
                metrics: vec![],
                dimensions: vec![],
                filters: vec![],
                history: vec![],
                spec_hint: None,
                selected_procedure: None,
                semantic_query: Default::default(),
                semantic_confidence: 0.0,
            })
        }
        "specifying" => {
            // Re-enter specifying by deserializing the stored intent.
            // stage_data is `{"intent": ..., "conversation_history": [...]}`;
            // fall back to treating the whole blob as the intent for backwards
            // compatibility with any suspended runs from before this change.
            let intent_value = if data.stage_data["intent"].is_object() {
                data.stage_data["intent"].clone()
            } else {
                data.stage_data.clone()
            };
            let intent: AnalyticsIntent =
                serde_json::from_value(intent_value).unwrap_or_else(|_| AnalyticsIntent {
                    raw_question: data.original_input.clone(),
                    summary: String::new(),
                    question_type: QuestionType::SingleValue,
                    metrics: vec![],
                    dimensions: vec![],
                    filters: vec![],
                    history: vec![],
                    spec_hint: None,
                    selected_procedure: None,
                    semantic_query: Default::default(),
                    semantic_confidence: 0.0,
                });
            // GeneralInquiry should never have entered Specifying, but if the
            // suspension data is corrupt/stale, re-triage via Clarifying rather
            // than forwarding to Specifying (which would attempt SQL generation).
            if intent.question_type == QuestionType::GeneralInquiry {
                ProblemState::Clarifying(intent)
            } else {
                ProblemState::Specifying(intent)
            }
        }
        "solving" => {
            // Solving is absorbed into specifying.  Resume into specifying
            // with the intent from the stored QuerySpec.
            let spec_value = if data.stage_data["spec"].is_object() {
                data.stage_data["spec"].clone()
            } else {
                data.stage_data.clone()
            };
            match serde_json::from_value::<QuerySpec>(spec_value) {
                Ok(spec) => ProblemState::Specifying(spec.intent),
                Err(_) => {
                    tracing::info!(
                        "[agentic-analytics] warn: failed to deserialize QuerySpec for \
                         solving resume; falling back to Clarifying"
                    );
                    ProblemState::Clarifying(AnalyticsIntent {
                        raw_question: data.original_input.clone(),
                        summary: String::new(),
                        question_type: QuestionType::SingleValue,
                        metrics: vec![],
                        dimensions: vec![],
                        filters: vec![],
                        history: vec![],
                        spec_hint: None,
                        selected_procedure: None,
                        semantic_query: Default::default(),
                        semantic_confidence: 0.0,
                    })
                }
            }
        }
        "interpreting" => {
            // Re-enter interpreting with a placeholder result.  The actual
            // result data is stored in stage_data["result_sets"] and will be
            // restored by `interpret_impl` when it detects the resume.
            ProblemState::Interpreting(AnalyticsResult::single(
                QueryResult {
                    columns: vec![],
                    rows: vec![],
                    total_row_count: 0,
                    truncated: false,
                },
                None,
            ))
        }
        other => {
            // Warn instead of panic so stale/corrupt suspension data doesn't
            // crash the server.  Fall back to the safest re-entry point.
            tracing::info!(
                "[agentic-analytics] warn: unsupported from_state for resume: '{other}'; \
                 falling back to Clarifying"
            );
            ProblemState::Clarifying(AnalyticsIntent {
                raw_question: data.original_input.clone(),
                summary: String::new(),
                question_type: QuestionType::SingleValue,
                metrics: vec![],
                dimensions: vec![],
                filters: vec![],
                history: vec![],
                spec_hint: None,
                selected_procedure: None,
                semantic_query: Default::default(),
                semantic_confidence: 0.0,
            })
        }
    }
}
