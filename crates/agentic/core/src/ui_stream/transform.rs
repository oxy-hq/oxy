//! Per-event core-event → [`UiBlock`] dispatch.
//!
//! Holds the bulk of [`super::UiTransformState::process_core`]: one match arm
//! per [`CoreEvent`] variant with the state bookkeeping (buffered back-edge
//! reason, pending LLM prompt-tokens, fan-out counters) that makes the
//! streamed output self-consistent for the UI.

use crate::events::{CoreEvent, DomainEvents};

use super::{UiBlock, UiTransformState};

impl<D: DomainEvents> UiTransformState<D> {
    pub(super) fn process_core(&mut self, ev: CoreEvent) -> Vec<UiBlock<D>> {
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
                cache_creation_input_tokens,
                cache_read_input_tokens,
                duration_ms,
                model,
                sub_spec_index,
                ..
            } => {
                let prompt_tokens = std::mem::take(&mut self.pending_prompt_tokens);
                vec![UiBlock::LlmUsage {
                    prompt_tokens,
                    output_tokens,
                    cache_creation_input_tokens,
                    cache_read_input_tokens,
                    duration_ms,
                    model,
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
                llm_duration_ms,
                sub_spec_index,
            } => {
                let mut blocks = vec![UiBlock::ToolCall {
                    name: name.clone(),
                    input,
                    llm_duration_ms,
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
            CoreEvent::InputResolved { answer, trace_id } => {
                vec![UiBlock::InputResolved { answer, trace_id }]
            }

            // ── Delegation — forwarded to UI as-is ───────────────────────
            CoreEvent::DelegationStarted { .. }
            | CoreEvent::DelegationEvent { .. }
            | CoreEvent::DelegationCompleted { .. } => {
                // Delegation events are persisted directly by the coordinator.
                // No UiBlock mapping needed — the SSE layer serializes them
                // from the raw event rows.
                vec![]
            }
        }
    }
}
