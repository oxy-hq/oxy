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

use crate::events::{CoreEvent, DomainEvents, Event, HumanInputQuestion, Outcome};

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

    /// The user answered; the pipeline is resuming.
    HumanInputResolved,

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
        duration_ms: u64,
        sub_spec_index: Option<usize>,
    },

    /// A domain-specific event that passes through unchanged.
    Domain(D),

    /// The pipeline completed successfully.
    Done,

    /// The pipeline terminated with an error.
    Error { message: String },
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
/// - dropping internal events (`ValidationPass/Fail`, `LlmStart`)
/// - mapping `LlmEnd` → `LlmUsage` with token counts and latency
pub struct UiTransformState<D: DomainEvents> {
    /// Converts a lower-cased state name to an optional one-line summary.
    summary_fn: Box<dyn Fn(&str) -> Option<String> + Send>,
    /// Converts a tool name to an optional enriched step summary emitted as
    /// [`UiBlock::StepSummaryUpdate`] alongside the tool call.
    tool_summary_fn: Box<dyn Fn(&str) -> Option<String> + Send>,
    /// Label of the most recent `StepStart`, echoed in the matching `StepEnd`.
    current_label: String,
    /// How many sub-specs were expected in the current fan-out (`FanOut` event).
    fan_out_total: Option<usize>,
    /// How many sub-specs have completed so far.
    sub_specs_done: usize,
    /// Reason from the most recent `BackEdge`, consumed by the next
    /// `StateEnter { "diagnosing" }` to produce a contextual summary.
    pending_back_reason: Option<String>,
    /// Prompt tokens from the most recent `LlmStart`, consumed by `LlmEnd`.
    pending_prompt_tokens: usize,
    _marker: PhantomData<D>,
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

    fn process_core(&mut self, ev: CoreEvent) -> Vec<UiBlock<D>> {
        match ev {
            // ── State transitions ──────────────────────────────────────────
            CoreEvent::StateEnter {
                state,
                sub_spec_index,
                ..
            } => {
                // For diagnosing, prefer the buffered back-edge reason as the
                // summary so the user sees *why* recovery is happening.
                let summary = if state == "diagnosing" {
                    self.pending_back_reason
                        .take()
                        .map(|r| format!("Recovering: {r}"))
                        .or_else(|| (self.summary_fn)(&state))
                } else {
                    (self.summary_fn)(&state)
                };
                self.current_label = state.clone();
                vec![UiBlock::StepStart {
                    label: state,
                    summary,
                    sub_spec_index,
                }]
            }

            CoreEvent::StateExit {
                outcome,
                sub_spec_index,
                ..
            } => {
                vec![UiBlock::StepEnd {
                    label: self.current_label.clone(),
                    outcome,
                    sub_spec_index,
                }]
            }

            // Buffer the reason; consumed when the diagnosing state enters.
            CoreEvent::BackEdge { reason, .. } => {
                self.pending_back_reason = Some(reason);
                vec![]
            }

            // ── LLM streaming ──────────────────────────────────────────────
            CoreEvent::LlmStart {
                prompt_tokens,
                sub_spec_index: _,
                ..
            } => {
                self.pending_prompt_tokens = prompt_tokens;
                vec![]
            }
            CoreEvent::LlmEnd {
                output_tokens,
                duration_ms,
                sub_spec_index,
                ..
            } => {
                let prompt_tokens = std::mem::take(&mut self.pending_prompt_tokens);
                vec![UiBlock::LlmUsage {
                    prompt_tokens,
                    output_tokens,
                    duration_ms,
                    sub_spec_index,
                }]
            }

            CoreEvent::LlmToken {
                token,
                sub_spec_index,
            } => vec![UiBlock::TextDelta {
                token,
                sub_spec_index,
            }],

            // ── Thinking ───────────────────────────────────────────────────
            CoreEvent::ThinkingStart { sub_spec_index, .. } => {
                vec![UiBlock::ThinkingStart { sub_spec_index }]
            }
            CoreEvent::ThinkingToken {
                token,
                sub_spec_index,
            } => vec![UiBlock::ThinkingToken {
                token,
                sub_spec_index,
            }],
            CoreEvent::ThinkingEnd { sub_spec_index, .. } => {
                vec![UiBlock::ThinkingEnd { sub_spec_index }]
            }

            // ── Tool use ───────────────────────────────────────────────────
            CoreEvent::ToolCall {
                name,
                input,
                sub_spec_index,
            } => {
                let mut blocks = vec![UiBlock::ToolCall {
                    name: name.clone(),
                    input,
                    sub_spec_index,
                }];
                if let Some(summary) = (self.tool_summary_fn)(&name) {
                    blocks.push(UiBlock::StepSummaryUpdate { summary });
                }
                blocks
            }

            CoreEvent::ToolResult {
                name,
                output,
                duration_ms,
                sub_spec_index,
            } => vec![UiBlock::ToolResult {
                name,
                output,
                duration_ms,
                sub_spec_index,
            }],

            // ── Validation — internal quality checks, not user-facing ──────
            CoreEvent::ValidationPass { .. } | CoreEvent::ValidationFail { .. } => vec![],

            // ── Fan-out ────────────────────────────────────────────────────
            CoreEvent::FanOut { spec_count, .. } => {
                self.fan_out_total = Some(spec_count);
                self.sub_specs_done = 0;
                vec![UiBlock::FanOutStart { total: spec_count }]
            }

            CoreEvent::SubSpecStart { index, total, .. } => {
                let label = format!("Query {} of {}", index + 1, total);
                self.current_label = label.clone();
                vec![UiBlock::SubSpecStart {
                    index,
                    total,
                    label,
                }]
            }

            CoreEvent::SubSpecEnd { index, .. } => {
                self.sub_specs_done += 1;
                let sub_end = UiBlock::SubSpecEnd {
                    index,
                    success: true,
                };
                if self.fan_out_total == Some(self.sub_specs_done) {
                    self.fan_out_total = None;
                    self.sub_specs_done = 0;
                    vec![sub_end, UiBlock::FanOutEnd { success: true }]
                } else {
                    vec![sub_end]
                }
            }

            // ── Terminal ───────────────────────────────────────────────────
            CoreEvent::Done { .. } => vec![UiBlock::Done],
            CoreEvent::Error { message, .. } => vec![UiBlock::Error { message }],

            // ── Human input ────────────────────────────────────────────────
            CoreEvent::AwaitingHumanInput { questions, .. } => {
                vec![UiBlock::AwaitingInput { questions }]
            }
            CoreEvent::HumanInputResolved { .. } => vec![UiBlock::HumanInputResolved],
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{CoreEvent, Event, Outcome};

    #[test]
    fn state_enter_maps_to_step_start() {
        let mut s: UiTransformState<()> = UiTransformState::new();
        let blocks = s.process(Event::Core(CoreEvent::StateEnter {
            state: "clarifying".into(),
            revision: 0,
            trace_id: "t".into(),
            sub_spec_index: None,
        }));
        assert_eq!(blocks.len(), 1);
        assert!(
            matches!(&blocks[0], UiBlock::StepStart { label, summary: None, .. } if label == "clarifying")
        );
    }

    #[test]
    fn summary_fn_populates_step_start_summary() {
        let mut s: UiTransformState<()> = UiTransformState::new().with_summary_fn(|state| {
            if state == "clarifying" {
                Some("Understanding your question".into())
            } else {
                None
            }
        });
        let blocks = s.process(Event::Core(CoreEvent::StateEnter {
            state: "clarifying".into(),
            revision: 0,
            trace_id: "t".into(),
            sub_spec_index: None,
        }));
        assert!(matches!(
            &blocks[0],
            UiBlock::StepStart { summary: Some(s), .. } if s == "Understanding your question"
        ));
    }

    #[test]
    fn summary_fn_returns_none_for_unknown_state() {
        let mut s: UiTransformState<()> = UiTransformState::new().with_summary_fn(|_| None);
        let blocks = s.process(Event::Core(CoreEvent::StateEnter {
            state: "executing".into(),
            revision: 0,
            trace_id: "t".into(),
            sub_spec_index: None,
        }));
        assert!(matches!(
            &blocks[0],
            UiBlock::StepStart { summary: None, .. }
        ));
    }

    #[test]
    fn state_exit_advanced_is_success() {
        let mut s: UiTransformState<()> = UiTransformState::new();
        s.process(Event::Core(CoreEvent::StateEnter {
            state: "clarifying".into(),
            revision: 0,
            trace_id: "t".into(),
            sub_spec_index: None,
        }));
        let blocks = s.process(Event::Core(CoreEvent::StateExit {
            state: "clarifying".into(),
            outcome: Outcome::Advanced,
            trace_id: "t".into(),
            sub_spec_index: None,
        }));
        assert!(matches!(
            &blocks[0],
            UiBlock::StepEnd {
                outcome: Outcome::Advanced,
                ..
            }
        ));
    }

    #[test]
    fn state_exit_failed_is_not_success() {
        let mut s: UiTransformState<()> = UiTransformState::new();
        s.process(Event::Core(CoreEvent::StateEnter {
            state: "executing".into(),
            revision: 0,
            trace_id: "t".into(),
            sub_spec_index: None,
        }));
        let blocks = s.process(Event::Core(CoreEvent::StateExit {
            state: "executing".into(),
            outcome: Outcome::Failed,
            trace_id: "t".into(),
            sub_spec_index: None,
        }));
        assert!(matches!(
            &blocks[0],
            UiBlock::StepEnd {
                outcome: Outcome::Failed,
                ..
            }
        ));
    }

    #[test]
    fn back_edge_buffered_and_used_in_diagnosing_summary() {
        let mut s: UiTransformState<()> = UiTransformState::new();

        // BackEdge should be dropped but buffer the reason.
        let dropped = s.process(Event::Core(CoreEvent::BackEdge {
            from: "executing".into(),
            to: "diagnosing".into(),
            reason: "invalid SQL syntax".into(),
            trace_id: "t".into(),
        }));
        assert!(dropped.is_empty());

        // Next StateEnter for "diagnosing" should consume the buffered reason.
        let blocks = s.process(Event::Core(CoreEvent::StateEnter {
            state: "diagnosing".into(),
            revision: 0,
            trace_id: "t".into(),
            sub_spec_index: None,
        }));
        assert!(matches!(
            &blocks[0],
            UiBlock::StepStart { summary: Some(s), .. } if s == "Recovering: invalid SQL syntax"
        ));

        // Reason should be cleared after consumption.
        let blocks2 = s.process(Event::Core(CoreEvent::StateEnter {
            state: "diagnosing".into(),
            revision: 1,
            trace_id: "t".into(),
            sub_spec_index: None,
        }));
        assert!(matches!(
            &blocks2[0],
            UiBlock::StepStart { summary: None, .. }
        ));
    }

    #[test]
    fn back_edge_non_diagnosing_state_uses_summary_fn() {
        let mut s: UiTransformState<()> = UiTransformState::new().with_summary_fn(|state| {
            if state == "solving" {
                Some("Generating a query".into())
            } else {
                None
            }
        });

        s.process(Event::Core(CoreEvent::BackEdge {
            from: "executing".into(),
            to: "solving".into(),
            reason: "bad column".into(),
            trace_id: "t".into(),
        }));

        // Non-diagnosing state: summary_fn wins, pending reason is still buffered.
        let blocks = s.process(Event::Core(CoreEvent::StateEnter {
            state: "solving".into(),
            revision: 1,
            trace_id: "t".into(),
            sub_spec_index: None,
        }));
        assert!(matches!(
            &blocks[0],
            UiBlock::StepStart { summary: Some(s), .. } if s == "Generating a query"
        ));
    }

    #[test]
    fn tool_call_without_summary_fn_emits_one_block() {
        let mut s: UiTransformState<()> = UiTransformState::new();
        let blocks = s.process(Event::Core(CoreEvent::ToolCall {
            name: "list_metrics".into(),
            input: "{}".into(),
            sub_spec_index: None,
        }));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], UiBlock::ToolCall { name, .. } if name == "list_metrics"));
    }

    #[test]
    fn tool_call_with_summary_fn_emits_summary_update() {
        let mut s: UiTransformState<()> = UiTransformState::new().with_tool_summary_fn(|tool| {
            if tool == "list_metrics" {
                Some("Checking available metrics".into())
            } else {
                None
            }
        });
        let blocks = s.process(Event::Core(CoreEvent::ToolCall {
            name: "list_metrics".into(),
            input: "{}".into(),
            sub_spec_index: None,
        }));
        assert_eq!(blocks.len(), 2);
        assert!(matches!(&blocks[0], UiBlock::ToolCall { .. }));
        assert!(matches!(
            &blocks[1],
            UiBlock::StepSummaryUpdate { summary } if summary == "Checking available metrics"
        ));
    }

    #[test]
    fn tool_call_no_match_in_summary_fn_emits_one_block() {
        let mut s: UiTransformState<()> = UiTransformState::new().with_tool_summary_fn(|_| None);
        let blocks = s.process(Event::Core(CoreEvent::ToolCall {
            name: "unknown_tool".into(),
            input: "{}".into(),
            sub_spec_index: None,
        }));
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn back_edge_is_dropped() {
        let mut s: UiTransformState<()> = UiTransformState::new();
        let blocks = s.process(Event::Core(CoreEvent::BackEdge {
            from: "executing".into(),
            to: "solving".into(),
            reason: "bad SQL".into(),
            trace_id: "t".into(),
        }));
        assert!(blocks.is_empty());
    }

    #[test]
    fn llm_token_maps_to_text_delta() {
        let mut s: UiTransformState<()> = UiTransformState::new();
        let blocks = s.process(Event::Core(CoreEvent::LlmToken {
            token: "hello".into(),
            sub_spec_index: None,
        }));
        assert!(matches!(&blocks[0], UiBlock::TextDelta { token, .. } if token == "hello"));
    }

    #[test]
    fn llm_start_dropped_end_emits_usage() {
        let mut s: UiTransformState<()> = UiTransformState::new();
        assert!(s
            .process(Event::Core(CoreEvent::LlmStart {
                state: "s".into(),
                prompt_tokens: 100,
                sub_spec_index: None,
            }))
            .is_empty());
        let blocks = s.process(Event::Core(CoreEvent::LlmEnd {
            state: "s".into(),
            output_tokens: 50,
            duration_ms: 1234,
            sub_spec_index: None,
        }));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(
            &blocks[0],
            UiBlock::LlmUsage {
                prompt_tokens: 100,
                output_tokens: 50,
                duration_ms: 1234,
                sub_spec_index: None,
            }
        ));
    }

    #[test]
    fn validation_events_dropped() {
        let mut s: UiTransformState<()> = UiTransformState::new();
        assert!(s
            .process(Event::Core(CoreEvent::ValidationPass { state: "s".into() }))
            .is_empty());
        assert!(s
            .process(Event::Core(CoreEvent::ValidationFail {
                state: "s".into(),
                errors: vec![]
            }))
            .is_empty());
    }

    #[test]
    fn fan_out_emits_fan_out_start() {
        let mut s: UiTransformState<()> = UiTransformState::new();
        let blocks = s.process(Event::Core(CoreEvent::FanOut {
            spec_count: 3,
            trace_id: "t".into(),
        }));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], UiBlock::FanOutStart { total: 3 }));
    }

    #[test]
    fn sub_spec_start_emits_sub_spec_start() {
        let mut s: UiTransformState<()> = UiTransformState::new();
        let blocks = s.process(Event::Core(CoreEvent::SubSpecStart {
            index: 0,
            total: 3,
            trace_id: "t".into(),
        }));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(
            &blocks[0],
            UiBlock::SubSpecStart { index: 0, total: 3, label }
            if label.contains("1") && label.contains("3")
        ));
    }

    #[test]
    fn last_sub_spec_end_emits_sub_spec_end_and_fan_out_end() {
        let mut s: UiTransformState<()> = UiTransformState::new();
        s.process(Event::Core(CoreEvent::FanOut {
            spec_count: 2,
            trace_id: "t".into(),
        }));
        s.process(Event::Core(CoreEvent::SubSpecStart {
            index: 0,
            total: 2,
            trace_id: "t".into(),
        }));
        s.process(Event::Core(CoreEvent::SubSpecEnd {
            index: 0,
            trace_id: "t".into(),
        }));
        let blocks = s.process(Event::Core(CoreEvent::SubSpecEnd {
            index: 1,
            trace_id: "t".into(),
        }));
        assert_eq!(blocks.len(), 2);
        assert!(matches!(
            &blocks[0],
            UiBlock::SubSpecEnd {
                index: 1,
                success: true
            }
        ));
        assert!(matches!(&blocks[1], UiBlock::FanOutEnd { success: true }));
    }

    #[test]
    fn intermediate_sub_spec_end_does_not_emit_fan_out_end() {
        let mut s: UiTransformState<()> = UiTransformState::new();
        s.process(Event::Core(CoreEvent::FanOut {
            spec_count: 3,
            trace_id: "t".into(),
        }));
        let blocks = s.process(Event::Core(CoreEvent::SubSpecEnd {
            index: 0,
            trace_id: "t".into(),
        }));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], UiBlock::SubSpecEnd { .. }));
    }

    #[test]
    fn step_end_label_mirrors_step_start() {
        let mut s: UiTransformState<()> = UiTransformState::new();
        s.process(Event::Core(CoreEvent::StateEnter {
            state: "executing".into(),
            revision: 0,
            trace_id: "t".into(),
            sub_spec_index: None,
        }));
        let blocks = s.process(Event::Core(CoreEvent::StateExit {
            state: "executing".into(),
            outcome: Outcome::Advanced,
            trace_id: "t".into(),
            sub_spec_index: None,
        }));
        assert!(
            matches!(&blocks[0], UiBlock::StepEnd { label, outcome: Outcome::Advanced, .. } if label == "executing")
        );
    }

    #[test]
    fn domain_event_passes_through() {
        let mut s: UiTransformState<()> = UiTransformState::new();
        let blocks = s.process(Event::Domain(()));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], UiBlock::Domain(())));
    }

    #[test]
    fn test_interleaved_sub_spec_events() {
        let mut s: UiTransformState<()> = UiTransformState::new();

        // FanOut { 3 }
        let blocks = s.process(Event::Core(CoreEvent::FanOut {
            spec_count: 3,
            trace_id: "t".into(),
        }));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], UiBlock::FanOutStart { total: 3 }));

        // SubSpecStart { 0 }
        let blocks = s.process(Event::Core(CoreEvent::SubSpecStart {
            index: 0,
            total: 3,
            trace_id: "t".into(),
        }));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(
            &blocks[0],
            UiBlock::SubSpecStart {
                index: 0,
                total: 3,
                ..
            }
        ));

        // SubSpecStart { 1 }
        let blocks = s.process(Event::Core(CoreEvent::SubSpecStart {
            index: 1,
            total: 3,
            trace_id: "t".into(),
        }));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(
            &blocks[0],
            UiBlock::SubSpecStart {
                index: 1,
                total: 3,
                ..
            }
        ));

        // StateEnter("solving", sub_spec_index: Some(0))
        let blocks = s.process(Event::Core(CoreEvent::StateEnter {
            state: "solving".into(),
            revision: 0,
            trace_id: "t".into(),
            sub_spec_index: Some(0),
        }));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(
            &blocks[0],
            UiBlock::StepStart {
                sub_spec_index: Some(0),
                ..
            }
        ));

        // StateEnter("solving", sub_spec_index: Some(1))
        let blocks = s.process(Event::Core(CoreEvent::StateEnter {
            state: "solving".into(),
            revision: 0,
            trace_id: "t".into(),
            sub_spec_index: Some(1),
        }));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(
            &blocks[0],
            UiBlock::StepStart {
                sub_spec_index: Some(1),
                ..
            }
        ));

        // SubSpecEnd { 0 } — first completion, no FanOutEnd yet
        let blocks = s.process(Event::Core(CoreEvent::SubSpecEnd {
            index: 0,
            trace_id: "t".into(),
        }));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], UiBlock::SubSpecEnd { index: 0, .. }));

        // SubSpecStart { 2 }
        let blocks = s.process(Event::Core(CoreEvent::SubSpecStart {
            index: 2,
            total: 3,
            trace_id: "t".into(),
        }));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(
            &blocks[0],
            UiBlock::SubSpecStart {
                index: 2,
                total: 3,
                ..
            }
        ));

        // SubSpecEnd { 1 } — second completion, no FanOutEnd yet
        let blocks = s.process(Event::Core(CoreEvent::SubSpecEnd {
            index: 1,
            trace_id: "t".into(),
        }));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], UiBlock::SubSpecEnd { index: 1, .. }));

        // SubSpecEnd { 2 } — third and final completion, FanOutEnd emitted
        let blocks = s.process(Event::Core(CoreEvent::SubSpecEnd {
            index: 2,
            trace_id: "t".into(),
        }));
        assert_eq!(blocks.len(), 2);
        assert!(matches!(&blocks[0], UiBlock::SubSpecEnd { index: 2, .. }));
        assert!(matches!(&blocks[1], UiBlock::FanOutEnd { success: true }));
    }

    #[test]
    fn test_sub_spec_index_propagated_to_ui_blocks() {
        let mut s: UiTransformState<()> = UiTransformState::new();
        let blocks = s.process(Event::Core(CoreEvent::StateEnter {
            state: "solving".into(),
            revision: 0,
            trace_id: "t".into(),
            sub_spec_index: Some(2),
        }));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(
            &blocks[0],
            UiBlock::StepStart { label, sub_spec_index: Some(2), .. } if label == "solving"
        ));
    }

    #[test]
    fn test_none_sub_spec_index_outside_fanout() {
        let mut s: UiTransformState<()> = UiTransformState::new();

        // StateEnter without fan-out context
        let blocks = s.process(Event::Core(CoreEvent::StateEnter {
            state: "clarifying".into(),
            revision: 0,
            trace_id: "t".into(),
            sub_spec_index: None,
        }));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(
            &blocks[0],
            UiBlock::StepStart {
                sub_spec_index: None,
                ..
            }
        ));

        // StateExit without fan-out context
        let blocks = s.process(Event::Core(CoreEvent::StateExit {
            state: "clarifying".into(),
            outcome: Outcome::Advanced,
            trace_id: "t".into(),
            sub_spec_index: None,
        }));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(
            &blocks[0],
            UiBlock::StepEnd {
                sub_spec_index: None,
                ..
            }
        ));

        // LlmToken without fan-out context
        let blocks = s.process(Event::Core(CoreEvent::LlmToken {
            token: "hi".into(),
            sub_spec_index: None,
        }));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(
            &blocks[0],
            UiBlock::TextDelta {
                sub_spec_index: None,
                ..
            }
        ));

        // ToolCall without fan-out context
        let blocks = s.process(Event::Core(CoreEvent::ToolCall {
            name: "some_tool".into(),
            input: "{}".into(),
            sub_spec_index: None,
        }));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(
            &blocks[0],
            UiBlock::ToolCall {
                sub_spec_index: None,
                ..
            }
        ));
    }
}
