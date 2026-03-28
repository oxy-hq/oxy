//! Human-in-the-loop (HITL) resume path helpers for the app builder domain.
//!
//! `problem_state_from_resume` reconstructs the correct [`ProblemState`] to
//! re-enter after the user answers a clarifying question.

use agentic_core::{human_input::SuspendedRunData, state::ProblemState};

use crate::types::{AppBuilderDomain, AppIntent, AppSpec};

/// Reconstruct the [`ProblemState`] to re-enter when resuming a suspended run.
///
/// Called from `DomainSolver::problem_state_from_resume` in the trait impl.
///
/// # Fallback on unknown `from_state`
///
/// Unknown or corrupt `from_state` values fall back to `Clarifying` (safest
/// re-entry point — `resume_data` is set so the triage phase will be skipped).
pub(super) fn problem_state_from_resume(data: &SuspendedRunData) -> ProblemState<AppBuilderDomain> {
    // Retry checkpoints: re-enter specifying so the solver can use
    // pre_computed_specs / pre_solved_sqls to skip already-done work.
    if data
        .stage_data
        .get("checkpoint_type")
        .and_then(|v| v.as_str())
        == Some("retry")
    {
        return problem_state_from_retry_checkpoint(data);
    }

    match data.from_state.as_str() {
        "clarifying" => {
            // Re-enter clarifying with the stored triage intent; clarify_impl
            // will detect resume_data is set and skip straight to the ground
            // phase, injecting the user's answer into the conversation history.
            let intent_value = data.stage_data["intent"].clone();
            let intent: AppIntent =
                serde_json::from_value(intent_value).unwrap_or_else(|_| AppIntent {
                    raw_request: data.original_input.clone(),
                    ..Default::default()
                });
            ProblemState::Clarifying(intent)
        }
        "specifying" => {
            // Re-enter specifying with the stored intent.
            let intent_value = data.stage_data["intent"].clone();
            let intent: AppIntent =
                serde_json::from_value(intent_value).unwrap_or_else(|_| AppIntent {
                    raw_request: data.original_input.clone(),
                    ..Default::default()
                });
            ProblemState::Specifying(intent)
        }
        "solving" => {
            // Re-enter solving with the stored spec.
            let spec_value = data.stage_data["spec"].clone();
            match serde_json::from_value::<AppSpec>(spec_value) {
                Ok(spec) => ProblemState::Solving(spec),
                Err(_) => {
                    eprintln!(
                        "[agentic-app-builder] warn: could not deserialize spec from \
                         solving suspension; falling back to Clarifying"
                    );
                    ProblemState::Clarifying(AppIntent {
                        raw_request: data.original_input.clone(),
                        ..Default::default()
                    })
                }
            }
        }
        other => {
            eprintln!(
                "[agentic-app-builder] warn: unsupported from_state for resume: '{other}'; \
                 falling back to Clarifying"
            );
            ProblemState::Clarifying(AppIntent {
                raw_request: data.original_input.clone(),
                ..Default::default()
            })
        }
    }
}

/// Reconstruct state for a retry checkpoint.
///
/// For all retry cases where specifying already succeeded (fanout failures,
/// interpreting failures), we re-enter at `Specifying` — the solver's
/// `pre_computed_specs` will short-circuit the LLM call and jump straight
/// to fanout.  For clarifying/specifying failures, we restart that state.
fn problem_state_from_retry_checkpoint(data: &SuspendedRunData) -> ProblemState<AppBuilderDomain> {
    let intent = data
        .stage_data
        .get("intent")
        .and_then(|v| serde_json::from_value::<AppIntent>(v.clone()).ok())
        .unwrap_or_else(|| AppIntent {
            raw_request: data.original_input.clone(),
            ..Default::default()
        });

    match data.from_state.as_str() {
        "clarifying" => ProblemState::Clarifying(intent),
        "specifying" => ProblemState::Specifying(intent),
        // Fanout failures (solving/executing) and interpreting failures:
        // re-enter at Specifying — the solver uses pre_computed_specs to
        // skip the LLM and jump directly into fanout.
        "solving" | "executing" | "interpreting" => ProblemState::Specifying(intent),
        other => {
            eprintln!(
                "[agentic-app-builder] warn: unsupported retry from_state: '{other}'; \
                 falling back to Clarifying"
            );
            ProblemState::Clarifying(intent)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_core::human_input::SuspendedRunData;
    use agentic_core::state::ProblemState;

    fn make_data(
        from_state: &str,
        stage_data: serde_json::Value,
        original_input: &str,
    ) -> SuspendedRunData {
        SuspendedRunData {
            from_state: from_state.to_string(),
            original_input: original_input.to_string(),
            trace_id: "trace-1".to_string(),
            stage_data,
            question: "Which time granularity do you want?".to_string(),
            suggestions: vec!["daily".to_string(), "monthly".to_string()],
        }
    }

    #[test]
    fn clarifying_with_stored_intent_reconstructs_correctly() {
        let stored_intent = AppIntent {
            raw_request: "build a sales dashboard".to_string(),
            app_name: Some("Sales Dashboard".to_string()),
            desired_metrics: vec!["revenue".to_string(), "orders".to_string()],
            desired_controls: vec!["date_filter".to_string()],
            mentioned_tables: vec!["sales".to_string()],
            ..Default::default()
        };

        let data = make_data(
            "clarifying",
            serde_json::json!({ "intent": serde_json::to_value(&stored_intent).unwrap() }),
            "build a sales dashboard",
        );

        let state = problem_state_from_resume(&data);
        match state {
            ProblemState::Clarifying(intent) => {
                assert_eq!(intent.raw_request, "build a sales dashboard");
                assert_eq!(intent.app_name, Some("Sales Dashboard".to_string()));
                assert_eq!(intent.desired_metrics, vec!["revenue", "orders"]);
                assert_eq!(intent.desired_controls, vec!["date_filter"]);
                assert_eq!(intent.mentioned_tables, vec!["sales"]);
            }
            _ => panic!("expected Clarifying, got something else"),
        }
    }

    #[test]
    fn clarifying_with_missing_intent_falls_back_to_raw_request() {
        // stage_data has no "intent" key — should fall back to original_input.
        let data = make_data("clarifying", serde_json::json!({}), "build a revenue app");

        let state = problem_state_from_resume(&data);
        match state {
            ProblemState::Clarifying(intent) => {
                assert_eq!(intent.raw_request, "build a revenue app");
                assert!(intent.app_name.is_none());
                assert!(intent.desired_metrics.is_empty());
            }
            _ => panic!("expected Clarifying"),
        }
    }

    #[test]
    fn unknown_from_state_falls_back_to_clarifying() {
        let data = make_data("unknown_state", serde_json::json!({}), "build a dashboard");

        let state = problem_state_from_resume(&data);
        match state {
            ProblemState::Clarifying(intent) => {
                assert_eq!(intent.raw_request, "build a dashboard");
            }
            _ => panic!("expected Clarifying fallback for unknown from_state"),
        }
    }
}
