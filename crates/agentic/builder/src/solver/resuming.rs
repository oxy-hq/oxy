use agentic_core::{human_input::SuspendedRunData, state::ProblemState};

use crate::types::{BuilderDomain, BuilderIntent, BuilderSpec};

pub(super) fn problem_state_from_resume(data: &SuspendedRunData) -> ProblemState<BuilderDomain> {
    // Always resume via Clarifying so the orchestrator sets run_ctx.intent
    // through the skip path (clarifying → specifying → solving). The builder
    // domain skips both clarifying and specifying via should_skip, so execution
    // still resumes at Solving — but intent is correctly captured along the way.
    let intent = match data.from_state.as_str() {
        "solving" => {
            // Reconstruct intent from the saved spec (same fields: question + history).
            match serde_json::from_value::<BuilderSpec>(data.stage_data["spec"].clone()) {
                Ok(spec) => BuilderIntent {
                    question: spec.question,
                    history: spec.history,
                },
                Err(e) => {
                    tracing::warn!("failed to deserialize BuilderSpec from stage_data: {e}");
                    BuilderIntent {
                        question: data.original_input.clone(),
                        history: vec![],
                    }
                }
            }
        }
        _ => BuilderIntent {
            question: data.original_input.clone(),
            history: vec![],
        },
    };
    ProblemState::Clarifying(intent)
}
