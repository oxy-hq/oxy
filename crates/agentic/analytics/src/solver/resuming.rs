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

/// Parse a delegation answer into an `AnalyticsResult` with proper
/// `QueryResult` entries.
///
/// The workflow's terminal-answer shape is `{task_name: result, ...}`
/// (an object keyed by task name in declaration order) per
/// `agentic_workflow::step_decider::build_final_answer`. Older runs
/// emitted a bare `Vec<Value>`; we still accept that shape for
/// rolling-upgrade safety.
///
/// Each value is one of:
/// - `{columns: [...], rows: [[...]]}` — tabular step result
/// - `{text: "..."}` — agent / formatter result
///
/// Both shapes become `QueryResultSet` entries the analytics
/// interpreter can scan when binding chart axes / answer text.
fn parse_delegation_answer(answer: &str) -> Option<AnalyticsResult> {
    let value: serde_json::Value = serde_json::from_str(answer).ok()?;
    let steps: Vec<&serde_json::Value> = match &value {
        serde_json::Value::Object(map) => map.values().collect(),
        serde_json::Value::Array(arr) => arr.iter().collect(),
        _ => return None,
    };
    if steps.is_empty() {
        return None;
    }

    let mut result_sets = Vec::new();
    for step in steps {
        if let Some(set) = step_to_result_set(step) {
            result_sets.push(set);
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

fn step_to_result_set(step: &serde_json::Value) -> Option<crate::types::QueryResultSet> {
    if let Some(columns_arr) = step.get("columns").and_then(|v| v.as_array()) {
        let columns: Vec<String> = columns_arr
            .iter()
            .filter_map(|c| c.as_str().map(str::to_string))
            .collect();
        let rows: Vec<QueryRow> = step
            .get("rows")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|r| {
                        r.as_array()
                            .map(|cells| QueryRow(cells.iter().map(json_cell_to_value).collect()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let total = rows.len() as u64;
        Some(crate::types::QueryResultSet {
            data: QueryResult {
                columns,
                rows,
                total_row_count: total,
                truncated: false,
            },
            summary: None,
        })
    } else {
        step.get("text")
            .and_then(|v| v.as_str())
            .map(|text| crate::types::QueryResultSet {
                data: QueryResult {
                    columns: vec!["result".to_string()],
                    rows: vec![QueryRow(vec![CellValue::Text(text.to_string())])],
                    total_row_count: 1,
                    truncated: false,
                },
                summary: None,
            })
    }
}

fn json_cell_to_value(cell: &serde_json::Value) -> CellValue {
    match cell {
        serde_json::Value::Number(n) => CellValue::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => CellValue::Text(s.clone()),
        serde_json::Value::Null => CellValue::Null,
        other => CellValue::Text(other.to_string()),
    }
}

/// Populate `RunContext` from suspension data so the orchestrator has
/// `intent` and `spec` when resuming mid-pipeline (e.g. from Executing).
pub(super) fn populate_resume_context(
    data: &SuspendedRunData,
    run_ctx: &mut RunContext<crate::AnalyticsDomain>,
) {
    // Restore intent from stage_data.
    if let Some(intent_val) = data.stage_data.get("intent")
        && let Ok(intent) = serde_json::from_value::<crate::AnalyticsIntent>(intent_val.clone())
    {
        run_ctx.intent = Some(intent);
    }

    // Restore spec from stage_data.
    if let Some(spec_val) = data.stage_data.get("spec")
        && let Ok(spec) = serde_json::from_value::<crate::QuerySpec>(spec_val.clone())
    {
        // If intent wasn't in stage_data, recover it from the spec.
        if run_ctx.intent.is_none() {
            run_ctx.intent = Some(spec.intent.clone());
        }
        run_ctx.spec = Some(spec);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: a workflow's terminal answer is now an object keyed
    /// by task name (per `agentic_workflow::step_decider::build_final_answer`).
    /// `parse_delegation_answer` must walk the object's values so the
    /// analytics interpreter sees one `QueryResultSet` per step. Without
    /// this, charts that bind to the workflow's table results render
    /// empty.
    #[test]
    fn object_shape_workflow_answer_produces_result_sets() {
        let answer = r#"{
            "query": { "columns": ["a", "b"], "rows": [[1, 2], [3, 4]] },
            "report": { "text": "summary" }
        }"#;
        let result = parse_delegation_answer(answer).expect("parsed");
        assert_eq!(result.results.len(), 2);
        // Tabular step round-trips columns and rows.
        let tabular = result
            .results
            .iter()
            .find(|r| r.data.columns == vec!["a", "b"])
            .expect("tabular result");
        assert_eq!(tabular.data.rows.len(), 2);
        // Text step lands as a single-cell result with the synthetic `result` column.
        let text = result
            .results
            .iter()
            .find(|r| r.data.columns == vec!["result"])
            .expect("text result");
        assert_eq!(text.data.rows.len(), 1);
        if let CellValue::Text(s) = &text.data.rows[0].0[0] {
            assert_eq!(s, "summary");
        } else {
            panic!("expected text cell");
        }
    }

    /// Older runs persisted before `build_final_answer` switched to the
    /// object shape emitted a bare `Vec<Value>`. Keep the array path
    /// working so a queued resume from before the shape change still
    /// renders correctly.
    #[test]
    fn array_shape_workflow_answer_still_parses() {
        let answer = r#"[
            { "columns": ["x"], "rows": [["v1"]] },
            { "text": "hi" }
        ]"#;
        let result = parse_delegation_answer(answer).expect("parsed");
        assert_eq!(result.results.len(), 2);
    }

    #[test]
    fn empty_inputs_return_none() {
        assert!(parse_delegation_answer("{}").is_none());
        assert!(parse_delegation_answer("[]").is_none());
        assert!(parse_delegation_answer("not-json").is_none());
    }
}
