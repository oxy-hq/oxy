//! SSE event serialization and deserialization for `Event<AnalyticsEvent>`,
//! plus [`UiBlock`] serialization for the frontend-facing stream.
//!
//! # Responsibilities
//!
//! - [`serialize`] — raw `Event<AnalyticsEvent>` → `(event_type, JSON)` for
//!   the bridge task to store in SQLite (unchanged from original).
//! - [`deserialize`] — `(event_type, JSON)` → `Event<AnalyticsEvent>` for
//!   the SSE handler to re-hydrate raw DB rows before transformation.
//! - [`serialize_ui_block`] — `UiBlock<AnalyticsEvent>` → `(event_type, JSON)`
//!   for the SSE handler to send to the frontend.

use agentic_analytics::AnalyticsEvent;
use agentic_builder::BuilderEvent;
use agentic_core::UiBlock;
use agentic_core::events::{CoreEvent, Event};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A serialized UI event used in REST responses (e.g. `list_runs_by_thread`).
/// Mirrors what the SSE stream emits, but as a plain JSON object.
#[derive(Serialize, Deserialize)]
pub struct UiEvent {
    pub seq: i64,
    pub event_type: String,
    pub payload: Value,
}

impl UiEvent {
    pub fn from_block(
        seq: i64,
        block: &UiBlock<AnalyticsEvent>,
        serializer: &mut UiBlockSerializer,
    ) -> Self {
        let (event_type, payload) = serializer.serialize_block(block);
        Self {
            seq,
            event_type,
            payload,
        }
    }
}

/// Stateful serializer that injects accumulated domain-event payloads into
/// `step_end` events as a `metadata` field, enabling frontend debugging.
///
/// One instance should be created per pipeline run (or per SSE connection when
/// replaying from DB), alongside [`UiTransformState`].
pub struct UiBlockSerializer {
    /// Domain-event payloads accumulated since the last `StepStart`, keyed by
    /// event type (e.g. `"intent_clarified"`). Cleared on each `StepEnd`.
    pending_domain: serde_json::Map<String, Value>,
}

impl Default for UiBlockSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl UiBlockSerializer {
    pub fn new() -> Self {
        Self {
            pending_domain: serde_json::Map::new(),
        }
    }

    /// Serialize a [`UiBlock`] into an `(event_type, JSON payload)` pair.
    ///
    /// `StepEnd` events have their payload enriched with a `metadata` object
    /// containing all domain events emitted during the preceding step.
    pub fn serialize_block(&mut self, block: &UiBlock<AnalyticsEvent>) -> (String, Value) {
        match block {
            UiBlock::StepStart {
                label,
                summary,
                sub_spec_index,
            } => {
                // Reset accumulated domain state for the new step.
                self.pending_domain.clear();
                (
                    "step_start".into(),
                    json!({ "label": label, "summary": summary, "sub_spec_index": sub_spec_index }),
                )
            }
            UiBlock::StepEnd {
                label,
                outcome,
                sub_spec_index,
            } => {
                let metadata = if self.pending_domain.is_empty() {
                    Value::Null
                } else {
                    Value::Object(std::mem::take(&mut self.pending_domain))
                };
                (
                    "step_end".into(),
                    json!({ "label": label, "outcome": outcome, "metadata": metadata, "sub_spec_index": sub_spec_index }),
                )
            }
            UiBlock::Domain(e) => {
                let (event_type, payload) = serialize_domain(e);
                // Only accumulate events that represent state outputs.
                // Intermediate events (schema_resolved, triage_completed,
                // analytics_validation_failed) are excluded.
                if e.is_accumulated() {
                    self.pending_domain
                        .insert(event_type.clone(), payload.clone());
                }
                (event_type, payload)
            }
            // All other variants are stateless — delegate to the free function.
            other => serialize_ui_block(other),
        }
    }
}

pub fn serialize(event: &Event<AnalyticsEvent>) -> (String, Value) {
    match event {
        Event::Core(e) => serialize_core(e),
        Event::Domain(e) => serialize_domain(e),
    }
}

/// Split a serde internally-tagged enum value into `(event_type, payload)`.
///
/// Internally tagged enums serialize as `{ "event_type": "...", ...fields }`.
/// This helper removes the tag key and returns it alongside the remaining object.
fn split_tagged(v: Value) -> (String, Value) {
    let Value::Object(mut obj) = v else {
        panic!("internally tagged enum always serializes to an object");
    };
    let event_type = obj
        .remove("event_type")
        .and_then(|v| {
            if let Value::String(s) = v {
                Some(s)
            } else {
                None
            }
        })
        .unwrap_or_default();
    (event_type, Value::Object(obj))
}

fn serialize_core(e: &CoreEvent) -> (String, Value) {
    split_tagged(serde_json::to_value(e).expect("CoreEvent serialization is infallible"))
}

fn serialize_domain(e: &AnalyticsEvent) -> (String, Value) {
    split_tagged(serde_json::to_value(e).expect("AnalyticsEvent serialization is infallible"))
}

/// Squash consecutive delta events in a REST replay response.
///
/// `text_delta` and `thinking_token` events are emitted one-per-token during
/// live streaming, which can result in thousands of entries for a single LLM
/// call.  For historical REST responses those individual events carry no extra
/// information — the frontend only needs the concatenated text.
///
/// This function collapses consecutive runs of the same delta event type into
/// a single event whose `token` field contains all tokens joined together.
/// The `seq` of the merged event is set to the *last* seq in the run so
/// clients can still use it as a cursor.  All other event types are left
/// untouched.
pub fn squash_deltas(events: Vec<UiEvent>) -> Vec<UiEvent> {
    let mut out: Vec<UiEvent> = Vec::with_capacity(events.len());

    for ev in events {
        match ev.event_type.as_str() {
            "text_delta" | "thinking_token" => {
                let token = ev.payload["token"].as_str().unwrap_or("").to_string();
                if let Some(last) = out.last_mut()
                    && last.event_type == ev.event_type
                {
                    // Append to the existing merged event.
                    let merged = last.payload["token"].as_str().unwrap_or("").to_string() + &token;
                    last.payload = serde_json::json!({ "token": merged });
                    last.seq = ev.seq;
                    continue;
                }
                out.push(ev);
            }
            _ => out.push(ev),
        }
    }

    out
}

/// Returns true for event types that signal the run has terminated.
///
/// Works for both raw event types (`"done"`, `"error"`) and UiBlock event
/// types — they intentionally use the same strings.
pub fn is_terminal(event_type: &str) -> bool {
    matches!(event_type, "done" | "error")
}

// ── Deserializer ───────────────────────────────────────────────────────────────

/// Re-hydrate a raw DB row back into a typed `Event<AnalyticsEvent>`.
///
/// This is the inverse of [`serialize`].  Returns `None` for event types that
/// are not recognised (e.g. rows written by a newer version of the server).
pub fn deserialize(event_type: &str, payload: &Value) -> Option<Event<AnalyticsEvent>> {
    // Reconstruct the internally-tagged object that serde expects:
    // `{ "event_type": "...", ...fields }`.
    let mut tagged = match payload {
        Value::Object(m) => m.clone(),
        _ => serde_json::Map::new(),
    };
    tagged.insert("event_type".into(), Value::String(event_type.to_string()));
    let tagged_val = Value::Object(tagged);

    // CoreEvent covers most event types; AnalyticsEvent handles domain events.
    if let Ok(core) = serde_json::from_value::<CoreEvent>(tagged_val.clone()) {
        return Some(Event::Core(core));
    }
    if let Ok(domain) = serde_json::from_value::<AnalyticsEvent>(tagged_val) {
        return Some(Event::Domain(domain));
    }
    None
}

// ── UiBlock serializer ─────────────────────────────────────────────────────────

/// Serialize a [`UiBlock`] into an `(event_type, JSON payload)` pair for SSE.
///
/// The `event_type` strings use snake_case and are stable across releases.
/// Domain events reuse the existing [`serialize_domain`] function so the
/// analytics payload format is defined in one place.
pub fn serialize_ui_block(block: &UiBlock<AnalyticsEvent>) -> (String, Value) {
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
        UiBlock::HumanInputResolved { answer } => {
            ("human_input_resolved".into(), json!({ "answer": answer }))
        }
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
            duration_ms,
            model,
            sub_spec_index,
        } => (
            "llm_usage".into(),
            json!({ "prompt_tokens": prompt_tokens, "output_tokens": output_tokens, "duration_ms": duration_ms, "model": model, "sub_spec_index": sub_spec_index }),
        ),
        UiBlock::Domain(e) => serialize_domain(e),
        UiBlock::Done => ("done".into(), json!({})),
        UiBlock::Error { message } => ("error".into(), json!({ "message": message })),
    }
}

/// Attempt to deserialize a raw DB row as a `BuilderEvent` and convert it
/// directly to `(event_type, payload)` pairs ready for SSE emission.
///
/// This is a fallback called when [`deserialize`] returns `None`, which
/// happens for builder domain events that are not part of the analytics schema.
/// Returns `None` for unrecognised event types.
pub fn deserialize_builder_ui(event_type: &str, payload: &Value) -> Option<Vec<(String, Value)>> {
    let mut tagged = match payload {
        Value::Object(m) => m.clone(),
        _ => serde_json::Map::new(),
    };
    tagged.insert("event_type".into(), Value::String(event_type.to_string()));
    let tagged_val = Value::Object(tagged);

    match serde_json::from_value::<BuilderEvent>(tagged_val) {
        Ok(BuilderEvent::ToolUsed { tool_name, summary }) => Some(vec![(
            "tool_used".into(),
            json!({ "tool_name": tool_name, "summary": summary }),
        )]),
        Ok(BuilderEvent::ProposedChange {
            file_path,
            description,
            new_content,
        }) => Some(vec![(
            "proposed_change".into(),
            json!({ "file_path": file_path, "description": description, "new_content": new_content }),
        )]),
        Err(_) => None,
    }
}

// ── Builder domain serialization ───────────────────────────────────────────────

/// Serialize a raw `Event<BuilderEvent>` for the bridge task to store in DB.
///
/// CoreEvents use the same format as analytics. BuilderEvent domain events
/// use the same internally-tagged pattern.
pub fn serialize_builder(event: &Event<BuilderEvent>) -> (String, Value) {
    match event {
        Event::Core(e) => serialize_core(e),
        Event::Domain(e) => {
            split_tagged(serde_json::to_value(e).expect("BuilderEvent serialization is infallible"))
        }
    }
}
