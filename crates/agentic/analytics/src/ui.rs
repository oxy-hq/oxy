//! UI helpers for the analytics domain.
//!
//! Provides the step-summary mapping used by [`UiTransformState`] to convert
//! internal pipeline state names into human-readable summaries.
//!
//! [`UiTransformState`]: agentic_core::UiTransformState

/// Map a lower-cased analytics pipeline state name to a user-friendly step summary.
///
/// Pass this function to
/// [`UiTransformState::with_summary_fn`](agentic_core::UiTransformState::with_summary_fn).
pub fn analytics_step_summary(state: &str) -> Option<String> {
    let s = match state {
        "clarifying" => "Understanding what you're asking for",
        "specifying" => "Grounding intent against the data model",
        "solving" => "Generating a query",
        "executing" => "Running query against the database",
        "interpreting" => "Synthesizing results into an answer",
        "diagnosing" => "Recovering from an error",
        _ => return None,
    };
    Some(s.to_string())
}

/// Map an analytics tool name to an enriched step summary.
///
/// Pass this function to
/// [`UiTransformState::with_tool_summary_fn`](agentic_core::UiTransformState::with_tool_summary_fn).
pub fn analytics_tool_summary(tool: &str, _input: &serde_json::Value) -> Option<String> {
    let s = match tool {
        "search_catalog" => "Searching catalog",
        "search_procedures" => "Searching procedures",
        "get_join_path" => "Resolving join path",
        "sample_columns" => "Sampling column values",
        "execute_preview" => "Previewing query",
        "render_chart" => "Rendering chart",
        _ => return None,
    };
    Some(s.to_string())
}
