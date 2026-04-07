use agentic_core::{human_input::SuspendedRunData, state::ProblemState};

use crate::types::{BuilderDomain, BuilderIntent, BuilderSpec};

pub(super) fn problem_state_from_resume(data: &SuspendedRunData) -> ProblemState<BuilderDomain> {
    match data.from_state.as_str() {
        "solving" => match serde_json::from_value::<BuilderSpec>(data.stage_data["spec"].clone()) {
            Ok(spec) => ProblemState::Solving(spec),
            Err(_) => ProblemState::Solving(BuilderSpec {
                question: data.original_input.clone(),
                history: vec![],
            }),
        },
        _ => ProblemState::Clarifying(BuilderIntent {
            question: data.original_input.clone(),
            history: vec![],
        }),
    }
}
