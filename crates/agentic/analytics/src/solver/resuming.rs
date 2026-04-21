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

use agentic_core::{
    human_input::SuspendedRunData,
    orchestrator::RunContext,
    result::{CellValue, QueryRow},
    state::ProblemState,
    tools::ToolDef,
};

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
pub(super) fn problem_state_from_resume(
    data: &SuspendedRunData,
    resume_answer: Option<&str>,
) -> ProblemState<AnalyticsDomain> {
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
        "executing" => {
            // Procedure delegation completed. Parse the workflow output
            // (JSON array of step results) into a real AnalyticsResult so
            // the frontend gets proper query_executed events with columns
            // and rows — just like inline procedure execution used to produce.
            let result = resume_answer
                .and_then(parse_delegation_answer)
                .unwrap_or_else(|| {
                    // Fallback: empty result if answer isn't available or parseable.
                    AnalyticsResult::single(
                        QueryResult {
                            columns: vec![],
                            rows: vec![],
                            total_row_count: 0,
                            truncated: false,
                        },
                        None,
                    )
                });
            ProblemState::Interpreting(result)
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

/// Parse a delegation answer (JSON array of step results) into an
/// `AnalyticsResult` with proper `QueryResult` entries.
fn parse_delegation_answer(answer: &str) -> Option<AnalyticsResult> {
    let steps: Vec<serde_json::Value> = serde_json::from_str(answer).ok()?;
    if steps.is_empty() {
        return None;
    }

    let mut result_sets = Vec::new();
    for step in &steps {
        if let Some(columns_arr) = step["columns"].as_array() {
            let columns: Vec<String> = columns_arr
                .iter()
                .filter_map(|c| c.as_str().map(str::to_string))
                .collect();
            let rows: Vec<QueryRow> = step["rows"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|r| {
                            r.as_array().map(|cells| {
                                QueryRow(
                                    cells
                                        .iter()
                                        .map(|cell| match cell {
                                            serde_json::Value::Number(n) => {
                                                CellValue::Number(n.as_f64().unwrap_or(0.0))
                                            }
                                            serde_json::Value::String(s) => {
                                                CellValue::Text(s.clone())
                                            }
                                            serde_json::Value::Null => CellValue::Null,
                                            other => CellValue::Text(other.to_string()),
                                        })
                                        .collect(),
                                )
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let total = rows.len() as u64;
            result_sets.push(crate::types::QueryResultSet {
                data: QueryResult {
                    columns,
                    rows,
                    total_row_count: total,
                    truncated: false,
                },
                summary: None,
            });
        } else if let Some(text) = step["text"].as_str() {
            result_sets.push(crate::types::QueryResultSet {
                data: QueryResult {
                    columns: vec!["result".to_string()],
                    rows: vec![QueryRow(vec![CellValue::Text(text.to_string())])],
                    total_row_count: 1,
                    truncated: false,
                },
                summary: None,
            });
        }
    }

    if result_sets.is_empty() {
        None
    } else {
        Some(AnalyticsResult {
            results: result_sets,
        })
    }
}

/// Populate `RunContext` from suspension data so the orchestrator has
/// `intent` and `spec` when resuming mid-pipeline (e.g. from Executing).
pub(super) fn populate_resume_context(
    data: &SuspendedRunData,
    run_ctx: &mut RunContext<crate::AnalyticsDomain>,
) {
    // Restore intent from stage_data.
    if let Some(intent_val) = data.stage_data.get("intent") {
        if let Ok(intent) = serde_json::from_value::<crate::AnalyticsIntent>(intent_val.clone()) {
            run_ctx.intent = Some(intent);
        }
    }

    // Restore spec from stage_data.
    if let Some(spec_val) = data.stage_data.get("spec") {
        if let Ok(spec) = serde_json::from_value::<crate::QuerySpec>(spec_val.clone()) {
            // If intent wasn't in stage_data, recover it from the spec.
            if run_ctx.intent.is_none() {
                run_ctx.intent = Some(spec.intent.clone());
            }
            run_ctx.spec = Some(spec);
        }
    }

    // Last resort: build a minimal intent from original_input so the
    // orchestrator's Done handler doesn't panic.
    if run_ctx.intent.is_none() {
        run_ctx.intent = Some(crate::AnalyticsIntent {
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
    }
}
