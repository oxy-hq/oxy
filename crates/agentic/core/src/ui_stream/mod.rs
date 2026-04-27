//! UI-facing event stream.
//!
//! Transforms raw [`Event<D>`] events emitted by the orchestrator into
//! [`UiBlock<D>`] values suitable for direct consumption by a frontend or
//! terminal renderer.  Internal FSM details (`StateEnter`, `StateExit`,
//! `BackEdge`, `ValidationPass/Fail`, `LlmStart/End`) are either mapped to
//! user-friendly equivalents or dropped entirely.
//!
//! # Usage
//!
//! ```rust,no_run
//! use agentic_core::{UiBlock, UiTransformState};
//! use agentic_core::events::Event;
//!
//! let mut state: UiTransformState<()> = UiTransformState::new();
//! // for each event received from the orchestrator / read from DB:
//! // let blocks: Vec<UiBlock<()>> = state.process(event);
//! ```

use std::marker::PhantomData;

use crate::events::{DomainEvents, Event, HumanInputQuestion, Outcome};

// ── UiBlock ────────────────────────────────────────────────────────────────────

/// A user-facing event block, derived from one or more raw [`Event`]s.
///
/// These are the only event types that reach the frontend or terminal renderer.
/// The variants deliberately avoid FSM terminology.
#[derive(Debug)]
pub enum UiBlock<D: DomainEvents = ()> {
    /// A logical pipeline step has started.
    ///
    /// `label` is a human-readable name (e.g. `"Analyzing"`, `"Running"`).
    /// `summary` is an optional one-line description of what this step does
    /// (e.g. `"Grounding intent against the data model"`).  May be enriched
    /// mid-step via [`StepSummaryUpdate`].
    ///
    /// [`StepSummaryUpdate`]: UiBlock::StepSummaryUpdate
    StepStart {
        label: String,
        summary: Option<String>,
        /// When inside a concurrent fan-out, identifies which sub-spec this
        /// block belongs to.  `None` outside fan-out.
        sub_spec_index: Option<usize>,
    },

    /// A logical pipeline step has ended.
    ///
    /// `label` mirrors the `StepStart` that opened this step.
    /// `outcome` carries the full [`Outcome`] so consumers can distinguish
    /// between a clean advance, a retry, a back-track, a fatal failure, and a
    /// suspension waiting for human input.
    StepEnd {
        label: String,
        outcome: Outcome,
        sub_spec_index: Option<usize>,
    },

    /// The current step's one-line summary was updated mid-step.
    ///
    /// Emitted when dynamic context (e.g. a tool call) allows a more specific
    /// description than the initial `StepStart.summary`.  The frontend should
    /// replace the currently displayed summary with this value.
    StepSummaryUpdate { summary: String },

    /// The model began emitting a thinking / reasoning block.
    ThinkingStart { sub_spec_index: Option<usize> },

    /// A single token of thinking text was received.
    ThinkingToken {
        token: String,
        sub_spec_index: Option<usize>,
    },

    /// The model finished the current thinking block.
    ThinkingEnd { sub_spec_index: Option<usize> },

    /// A tool was invoked.
    ToolCall {
        name: String,
        input: String,
        /// LLM inference time for the round that produced this tool call (ms).
        llm_duration_ms: u64,
        sub_spec_index: Option<usize>,
    },

    /// A tool invocation completed.
    ToolResult {
        name: String,
        output: String,
        duration_ms: u64,
        sub_spec_index: Option<usize>,
    },

    /// A fragment of the model's text response.
    TextDelta {
        token: String,
        sub_spec_index: Option<usize>,
    },

    /// The pipeline is waiting for a human reply.
    AwaitingInput { questions: Vec<HumanInputQuestion> },

    /// The suspended run received an answer; the pipeline is resuming.
    InputResolved { answer: String, trace_id: String },

    /// A fan-out group has started; the UI should prepare `total` cards for navigation.
    ///
    /// Followed by `total` pairs of [`SubSpecStart`] / [`SubSpecEnd`], then a
    /// single [`FanOutEnd`].
    ///
    /// [`SubSpecStart`]: UiBlock::SubSpecStart
    /// [`SubSpecEnd`]: UiBlock::SubSpecEnd
    /// [`FanOutEnd`]: UiBlock::FanOutEnd
    FanOutStart { total: usize },

    /// A single sub-spec within a fan-out group has started.
    ///
    /// `index` is zero-based.  `label` is a display string for the card header
    /// (e.g. `"Query 1 of 3"`).
    SubSpecStart {
        index: usize,
        total: usize,
        label: String,
    },

    /// A single sub-spec within a fan-out group has ended.
    SubSpecEnd { index: usize, success: bool },

    /// All sub-specs in the fan-out group have completed.
    FanOutEnd { success: bool },

    /// LLM usage stats for the current step (token counts and latency).
    LlmUsage {
        prompt_tokens: usize,
        output_tokens: usize,
        /// Tokens written to the prompt cache (Anthropic only).
        cache_creation_input_tokens: usize,
        /// Tokens read from the prompt cache (Anthropic only).  All three
        /// counters are disjoint — Anthropic's `input_tokens` is the
        /// uncached remainder, NOT a superset.  Total prompt size is the
        /// sum of all three; cache-hit ratio is
        /// `cache_read / (prompt_tokens + cache_creation + cache_read)`.
        cache_read_input_tokens: usize,
        duration_ms: u64,
        /// The model identifier used for this LLM call.
        model: String,
        sub_spec_index: Option<usize>,
    },

    /// A domain-specific event that passes through unchanged.
    Domain(D),

    /// The pipeline completed successfully.
    Done,

    /// The pipeline terminated with an error.
    Error { message: String },
}

/// Serialize a [`UiBlock`] into an `(event_type, JSON payload)` pair.
///
/// Works with any domain event type that implements `Serialize`.
/// Domain events are serialized using serde's internally-tagged format and
/// then split into `(event_type, payload)`.
pub fn serialize_ui_block<D: DomainEvents>(block: &UiBlock<D>) -> (String, serde_json::Value) {
    use serde_json::json;
    match block {
        UiBlock::StepStart {
            label,
            summary,
            sub_spec_index,
        } => (
            "step_start".into(),
            json!({ "label": label, "summary": summary, "sub_spec_index": sub_spec_index }),
        ),
        UiBlock::StepEnd {
            label,
            outcome,
            sub_spec_index,
        } => (
            "step_end".into(),
            json!({ "label": label, "outcome": outcome, "sub_spec_index": sub_spec_index }),
        ),
        UiBlock::StepSummaryUpdate { summary } => {
            ("step_summary_update".into(), json!({ "summary": summary }))
        }
        UiBlock::ThinkingStart { sub_spec_index } => (
            "thinking_start".into(),
            json!({ "sub_spec_index": sub_spec_index }),
        ),
        UiBlock::ThinkingToken {
            token,
            sub_spec_index,
        } => (
            "thinking_token".into(),
            json!({ "token": token, "sub_spec_index": sub_spec_index }),
        ),
        UiBlock::ThinkingEnd { sub_spec_index } => (
            "thinking_end".into(),
            json!({ "sub_spec_index": sub_spec_index }),
        ),
        UiBlock::ToolCall {
            name,
            input,
            llm_duration_ms,
            sub_spec_index,
        } => (
            "tool_call".into(),
            json!({ "name": name, "input": input, "llm_duration_ms": llm_duration_ms, "sub_spec_index": sub_spec_index }),
        ),
        UiBlock::ToolResult {
            name,
            output,
            duration_ms,
            sub_spec_index,
        } => (
            "tool_result".into(),
            json!({ "name": name, "output": output, "duration_ms": duration_ms, "sub_spec_index": sub_spec_index }),
        ),
        UiBlock::TextDelta {
            token,
            sub_spec_index,
        } => (
            "text_delta".into(),
            json!({ "token": token, "sub_spec_index": sub_spec_index }),
        ),
        UiBlock::AwaitingInput { questions } => (
            "awaiting_input".into(),
            json!({
                "questions": questions.iter().map(|q| json!({
                    "prompt": q.prompt,
                    "suggestions": q.suggestions,
                })).collect::<Vec<_>>(),
            }),
        ),
        UiBlock::InputResolved { answer, trace_id } => (
            "input_resolved".into(),
            json!({ "answer": answer, "trace_id": trace_id }),
        ),
        UiBlock::FanOutStart { total } => ("fan_out_start".into(), json!({ "total": total })),
        UiBlock::SubSpecStart {
            index,
            total,
            label,
        } => (
            "sub_spec_start".into(),
            json!({ "index": index, "total": total, "label": label }),
        ),
        UiBlock::SubSpecEnd { index, success } => (
            "sub_spec_end".into(),
            json!({ "index": index, "success": success }),
        ),
        UiBlock::FanOutEnd { success } => ("fan_out_end".into(), json!({ "success": success })),
        UiBlock::LlmUsage {
            prompt_tokens,
            output_tokens,
            cache_creation_input_tokens,
            cache_read_input_tokens,
            duration_ms,
            model,
            sub_spec_index,
        } => (
            "llm_usage".into(),
            json!({
                "prompt_tokens": prompt_tokens,
                "output_tokens": output_tokens,
                "cache_creation_input_tokens": cache_creation_input_tokens,
                "cache_read_input_tokens": cache_read_input_tokens,
                "duration_ms": duration_ms,
                "model": model,
                "sub_spec_index": sub_spec_index,
            }),
        ),
        UiBlock::Domain(e) => {
            let val = serde_json::to_value(e).unwrap_or(serde_json::Value::Null);
            if let serde_json::Value::Object(mut obj) = val {
                let et = obj
                    .remove("event_type")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| "domain".into());
                (et, serde_json::Value::Object(obj))
            } else {
                ("domain".into(), val)
            }
        }
        UiBlock::Done => ("done".into(), json!({})),
        UiBlock::Error { message } => ("error".into(), json!({ "message": message })),
    }
}

// ── UiTransformState ──────────────────────────────────────────────────────────

/// Stateful transformer: converts [`Event<D>`] → [`Vec<UiBlock<D>>`].
///
/// One instance should be created per pipeline run (or per SSE connection when
/// replaying from DB).  Call [`process`](UiTransformState::process) for every
/// event in sequence order.
///
/// The transformer is responsible for:
/// - mapping `StateEnter` → `StepStart` using the raw state name as label, plus an optional summary
/// - mapping `StateExit`  → `StepEnd`
/// - buffering `BackEdge.reason` and injecting it as the diagnosing step summary
/// - emitting `StepSummaryUpdate` when a tool call provides richer context
/// - emitting dedicated fan-out variants (`FanOutStart`, `SubSpecStart/End`,
///   `FanOutEnd`) instead of generic `StepStart/End`
/// - dropping internal events (`ValidationPass/Fail`)
/// - mapping each `LlmStart`/`LlmEnd` pair → `LlmUsage` with per-round token counts and latency
pub struct UiTransformState<D: DomainEvents> {
    /// Converts a lower-cased state name to an optional one-line summary.
    pub(super) summary_fn: Box<dyn Fn(&str) -> Option<String> + Send>,
    /// Converts a tool name to an optional enriched step summary emitted as
    /// [`UiBlock::StepSummaryUpdate`] alongside the tool call.
    pub(super) tool_summary_fn: Box<dyn Fn(&str) -> Option<String> + Send>,
    /// Label of the most recent `StepStart`, echoed in the matching `StepEnd`.
    pub(super) current_label: String,
    /// How many sub-specs were expected in the current fan-out (`FanOut` event).
    pub(super) fan_out_total: Option<usize>,
    /// How many sub-specs have completed so far.
    pub(super) sub_specs_done: usize,
    /// Reason from the most recent `BackEdge`, consumed by the next
    /// `StateEnter { "diagnosing" }` to produce a contextual summary.
    pub(super) pending_back_reason: Option<String>,
    /// Prompt tokens from the most recent `LlmStart`, consumed by `LlmEnd`.
    pub(super) pending_prompt_tokens: usize,
    pub(super) _marker: PhantomData<D>,
}

impl<D: DomainEvents> Default for UiTransformState<D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D: DomainEvents> UiTransformState<D> {
    /// Create a new transformer.
    ///
    /// Use [`with_summary_fn`] and [`with_tool_summary_fn`] to enable richer
    /// one-line summaries and dynamic enrichment.
    ///
    /// [`with_summary_fn`]: UiTransformState::with_summary_fn
    /// [`with_tool_summary_fn`]: UiTransformState::with_tool_summary_fn
    pub fn new() -> Self {
        Self {
            summary_fn: Box::new(|_| None),
            tool_summary_fn: Box::new(|_| None),
            current_label: String::new(),
            fan_out_total: None,
            sub_specs_done: 0,
            pending_back_reason: None,
            pending_prompt_tokens: 0,
            _marker: PhantomData,
        }
    }

    /// Set the per-state base summary generator.
    ///
    /// Called once on `StateEnter`.  Return `Some(text)` for states that have a
    /// meaningful description, `None` to omit the summary for that state.
    pub fn with_summary_fn(mut self, f: impl Fn(&str) -> Option<String> + Send + 'static) -> Self {
        self.summary_fn = Box::new(f);
        self
    }

    /// Set the tool-name enrichment generator.
    ///
    /// Called on every `ToolCall` event.  Return `Some(text)` to emit a
    /// [`UiBlock::StepSummaryUpdate`] alongside the tool call, updating the
    /// frontend's current step summary with a more specific description.
    pub fn with_tool_summary_fn(
        mut self,
        f: impl Fn(&str) -> Option<String> + Send + 'static,
    ) -> Self {
        self.tool_summary_fn = Box::new(f);
        self
    }

    /// Process a single raw event, returning zero or more UI blocks.
    ///
    /// Takes ownership of the event so that `Domain(D)` values can be moved
    /// into [`UiBlock::Domain`] without requiring `D: Clone`.
    pub fn process(&mut self, event: Event<D>) -> Vec<UiBlock<D>> {
        match event {
            Event::Core(ev) => self.process_core(ev),
            Event::Domain(d) => vec![UiBlock::Domain(d)],
        }
    }
}

pub mod transform;

#[cfg(test)]
mod tests;
