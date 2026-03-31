use std::pin::Pin;

use async_stream::stream;
use async_trait::async_trait;
use futures_core::Stream;
use serde_json::{Value, json};

use agentic_core::tools::ToolDef;

use super::sse::{ApiError, pop_sse_event, sse_data};
use super::{
    Chunk, ContentBlock, LlmError, LlmProvider, ResponseSchema, StopReason, ThinkingConfig,
    ToolCallChunk, Usage,
};

// ── CoT thinking parser ───────────────────────────────────────────────────────

/// State machine that separates chain-of-thought `<think>…</think>` spans
/// from ordinary text in a streaming response.
///
/// Models that support CoT (DeepSeek-style or prompted) emit:
/// ```text
/// <think>
/// step-by-step reasoning here…
/// </think>
/// The final answer to the question is…
/// ```
///
/// The parser yields [`ThinkChunk`]s so the caller can emit the right
/// [`Chunk`] variant without buffering the entire response.
#[derive(Default)]
struct ThinkParser {
    state: ParseState,
}

#[derive(Default)]
enum ParseState {
    /// We are outside any `<think>` block — emitting normal text.
    #[default]
    Text,
    /// We are inside a `<think>…</think>` block.
    Think,
    /// We saw a `<` and are buffering to decide whether it is a tag.
    Tag {
        /// Characters buffered after the `<`.
        buf: String,
        /// Whether we entered `Tag` from inside a `<think>` block.
        was_thinking: bool,
    },
}

enum ThinkChunk {
    Text(String),
    Think(String),
}

impl ThinkParser {
    /// Feed the next delta string from the stream; returns zero or more chunks.
    fn push(&mut self, s: &str) -> Vec<ThinkChunk> {
        let mut out: Vec<ThinkChunk> = Vec::new();
        let mut pending = String::new();

        for ch in s.chars() {
            match &mut self.state {
                ParseState::Text => {
                    if ch == '<' {
                        // Flush pending text before entering tag-scan.
                        if !pending.is_empty() {
                            out.push(ThinkChunk::Text(std::mem::take(&mut pending)));
                        }
                        self.state = ParseState::Tag {
                            buf: String::new(),
                            was_thinking: false,
                        };
                    } else {
                        pending.push(ch);
                    }
                }
                ParseState::Think => {
                    if ch == '<' {
                        if !pending.is_empty() {
                            out.push(ThinkChunk::Think(std::mem::take(&mut pending)));
                        }
                        self.state = ParseState::Tag {
                            buf: String::new(),
                            was_thinking: true,
                        };
                    } else {
                        pending.push(ch);
                    }
                }
                ParseState::Tag { buf, was_thinking } => {
                    buf.push(ch);
                    let was_thinking = *was_thinking;
                    // Check against known tag suffixes (after the leading `<`).
                    if buf == "think>" {
                        self.state = ParseState::Think;
                    } else if buf == "/think>" {
                        self.state = ParseState::Text;
                    } else if !("think>".starts_with(buf.as_str())
                        || "/think>".starts_with(buf.as_str()))
                    {
                        // Not a recognised tag — flush `<` + buf as the original state.
                        let raw = format!("<{}", buf);
                        if was_thinking {
                            out.push(ThinkChunk::Think(raw));
                            self.state = ParseState::Think;
                        } else {
                            out.push(ThinkChunk::Text(raw));
                            self.state = ParseState::Text;
                        }
                    }
                    // else: still a valid prefix, keep buffering
                }
            }
        }

        // Flush whatever is in `pending`.
        if !pending.is_empty() {
            match self.state {
                ParseState::Think => out.push(ThinkChunk::Think(pending)),
                _ => out.push(ThinkChunk::Text(pending)),
            }
        }

        out
    }

    /// Flush any buffered tag-scan state at end-of-stream.
    fn finish(self) -> Option<ThinkChunk> {
        if let ParseState::Tag { buf, was_thinking } = self.state {
            if buf.is_empty() {
                return None;
            }
            let raw = format!("<{buf}");
            return Some(if was_thinking {
                ThinkChunk::Think(raw)
            } else {
                ThinkChunk::Text(raw)
            });
        }
        None
    }
}

// ── System-prompt CoT injection ───────────────────────────────────────────────

const COT_PREFIX: &str = "Before responding, reason through the problem step by step inside \
<think>...</think> tags. After </think>, give your final answer.\n\n";

fn inject_cot(system: &str) -> String {
    format!("{COT_PREFIX}{system}")
}

// ── OpenAiCompatProvider ──────────────────────────────────────────────────────

/// OpenAI Chat Completions API provider for OpenAI-compatible backends
/// (Ollama, vLLM, LM Studio, etc.).
///
/// Uses the `/v1/chat/completions` endpoint which is the de-facto standard for
/// locally-hosted LLMs.  Supports:
///
/// - **Streaming** via SSE `choices[0].delta` events.
/// - **Tool calling** in the standard OpenAI function-call format.
/// - **Structured output** via `response_format: {type: "json_schema", ...}`.
/// - **Chain-of-Thought thinking** (any [`ThinkingConfig`] other than
///   [`ThinkingConfig::Disabled`]): the system prompt is prefixed with a CoT
///   instruction and `<think>…</think>` spans in the response are parsed into
///   [`Chunk::ThinkingSummary`] events.
pub struct OpenAiCompatProvider {
    api_key: String,
    model: String,
    /// Base URL of the Chat Completions endpoint, e.g.
    /// `http://localhost:11434/v1` (Ollama) or `http://host:8000/v1` (vLLM).
    base_url: String,
    client: reqwest::Client,
}

impl OpenAiCompatProvider {
    /// Create a provider.
    ///
    /// `base_url` should point at the root of the OpenAI-compat API, e.g.
    /// `"http://localhost:11434/v1"`.  The `/chat/completions` path is
    /// appended automatically.
    pub fn new(
        api_key: impl Into<String>,
        model: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        let mut base = base_url.into();
        // Normalise: strip trailing slash.
        while base.ends_with('/') {
            base.pop();
        }
        Self {
            api_key: api_key.into(),
            model: model.into(),
            base_url: base,
            client: reqwest::Client::new(),
        }
    }

    fn completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatProvider {
    async fn stream(
        &self,
        system: &str,
        messages: &[Value],
        tools: &[ToolDef],
        thinking: &ThinkingConfig,
        response_schema: Option<&ResponseSchema>,
        _max_tokens_override: Option<u32>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Chunk, LlmError>> + Send>>, LlmError> {
        // Inject CoT prefix when thinking is enabled.
        let effective_system = match thinking {
            ThinkingConfig::Disabled => system.to_string(),
            _ => inject_cot(system),
        };

        // Build the messages array in Chat Completions format.
        // The system prompt becomes a {"role":"system"} entry, followed by
        // the conversation messages.  Messages that arrive here are already in
        // OpenAI Chat Completions format (role/content or role/tool_calls).
        let mut chat_messages: Vec<Value> = Vec::new();
        if !effective_system.is_empty() {
            chat_messages.push(json!({"role": "system", "content": effective_system}));
        }
        for msg in messages {
            chat_messages.push(msg.clone());
        }

        // Build tools array (Chat Completions format).
        let tools_json: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters
                    }
                })
            })
            .collect();

        let mut body = json!({
            "model": self.model,
            "messages": chat_messages,
            "stream": true,
            "stream_options": {"include_usage": true}
        });

        if !tools_json.is_empty() {
            body["tools"] = json!(tools_json);
            body["tool_choice"] = json!("auto");
        }

        // Structured output.
        if let Some(schema) = response_schema {
            if tools.is_empty() {
                body["response_format"] = json!({
                    "type": "json_schema",
                    "json_schema": {
                        "name": schema.name,
                        "schema": schema.schema,
                        "strict": true
                    }
                });
            } else {
                // When there are real tools we can't use response_format at the
                // same time; inject a synthetic tool for structured output
                // (same strategy as Anthropic provider).
                let mut schema_tools = tools_json.clone();
                schema_tools.push(json!({
                    "type": "function",
                    "function": {
                        "name": schema.name,
                        "description": "Return the structured response.",
                        "parameters": schema.schema
                    }
                }));
                body["tools"] = json!(schema_tools);
            }
        }

        let url = self.completions_url();
        let mut req = self
            .client
            .post(&url)
            .header("content-type", "application/json");

        if !self.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Http(e.to_string()))?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            let text = response.text().await.unwrap_or_default();
            return Err(LlmError::Auth(text));
        }
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            if let Ok(api_err) = serde_json::from_str::<ApiError>(&text) {
                return Err(LlmError::Http(api_err.error.message));
            }
            return Err(LlmError::Http(format!("HTTP {status}: {text}")));
        }

        let enable_cot = !matches!(thinking, ThinkingConfig::Disabled);

        let s = stream! {
            use tokio_stream::StreamExt as _;

            let mut sse_buf = String::new();
            // tool_index → (id, name, accumulated_args)
            let mut tool_accum: std::collections::BTreeMap<u64, (String, String, String)> =
                std::collections::BTreeMap::new();
            let mut usage = Usage::default();
            let mut parser = ThinkParser::default();
            let mut in_thinking = false;

            let mut byte_stream = response.bytes_stream();

            'outer: while let Some(bytes_result) = byte_stream.next().await {
                let bytes = match bytes_result {
                    Ok(b) => b,
                    Err(e) => {
                        yield Err(LlmError::Http(e.to_string()));
                        return;
                    }
                };
                sse_buf.push_str(&String::from_utf8_lossy(&bytes));

                loop {
                    let event_text = match pop_sse_event(&mut sse_buf) {
                        Some(e) => e,
                        None => break,
                    };

                    let data = match sse_data(&event_text) {
                        Some(d) if !d.is_empty() => d,
                        _ => continue,
                    };

                    if data == "[DONE]" {
                        // Flush any partial tag buffer at end-of-stream.
                        if let Some(chunk) = parser.finish() {
                            match chunk {
                                ThinkChunk::Think(t) => yield Ok(Chunk::ThinkingSummary(t)),
                                ThinkChunk::Text(t) => {
                                    if !t.is_empty() {
                                        yield Ok(Chunk::Text(t));
                                    }
                                }
                            }
                        }
                        break 'outer;
                    }

                    let ev: Value = match serde_json::from_str(data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    // Usage chunk (last chunk with stream_options.include_usage).
                    if let Some(u) = ev.get("usage") {
                        let stop_reason = ev["choices"]
                            .get(0)
                            .and_then(|c| c["finish_reason"].as_str())
                            .map(|r| if r == "length" { StopReason::MaxTokens } else { StopReason::EndTurn })
                            .unwrap_or(StopReason::EndTurn);
                        usage = Usage {
                            input_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as usize,
                            output_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as usize,
                            stop_reason,
                        };
                    }

                    // Delta chunk.
                    let Some(choice) = ev["choices"].get(0) else { continue };
                    let delta = &choice["delta"];

                    // Text content delta.
                    if let Some(content) = delta["content"].as_str()
                        && !content.is_empty() {
                            if enable_cot {
                                for chunk in parser.push(content) {
                                    match chunk {
                                        ThinkChunk::Think(t) => {
                                            in_thinking = true;
                                            yield Ok(Chunk::ThinkingSummary(t));
                                        }
                                        ThinkChunk::Text(t) => {
                                            if in_thinking {
                                                in_thinking = false;
                                            }
                                            if !t.is_empty() {
                                                yield Ok(Chunk::Text(t));
                                            }
                                        }
                                    }
                                }
                            } else {
                                yield Ok(Chunk::Text(content.to_string()));
                            }
                        }

                    // Tool call deltas.
                    if let Some(tool_calls) = delta["tool_calls"].as_array() {
                        for tc in tool_calls {
                            let idx = tc["index"].as_u64().unwrap_or(0);
                            let entry = tool_accum.entry(idx).or_insert_with(|| {
                                (String::new(), String::new(), String::new())
                            });
                            if let Some(id) = tc["id"].as_str() {
                                entry.0 = id.to_string();
                            }
                            if let Some(name) = tc["function"]["name"].as_str() {
                                entry.1 = name.to_string();
                            }
                            if let Some(args) = tc["function"]["arguments"].as_str() {
                                entry.2.push_str(args);
                            }
                        }
                    }

                    // On finish_reason, flush accumulated tool calls.
                    if let Some(reason) = choice["finish_reason"].as_str() {
                        if reason == "tool_calls" || reason == "stop" {
                            for (_idx, (call_id, name, args)) in std::mem::take(&mut tool_accum) {
                                let fn_input: Value =
                                    serde_json::from_str(&args).unwrap_or(Value::Null);
                                yield Ok(Chunk::ToolCall(ToolCallChunk {
                                    id: call_id,
                                    name,
                                    input: fn_input,
                                    provider_data: None,
                                }));
                            }
                        }
                        if reason == "stop" || reason == "length" {
                            let stop_reason = if reason == "length" {
                                StopReason::MaxTokens
                            } else {
                                StopReason::EndTurn
                            };
                            // Update usage stop_reason if the usage chunk hasn't
                            // arrived yet (some backends send finish_reason before usage).
                            if usage.stop_reason == StopReason::EndTurn {
                                usage.stop_reason = stop_reason;
                            }
                        }
                    }
                }
            }

            yield Ok(Chunk::Done(usage));
        };

        Ok(Box::pin(s))
    }

    /// Serialize content blocks as Chat Completions assistant message(s).
    ///
    /// Returns a single `{"role":"assistant", ...}` object.  Tool calls from
    /// the previous turn are expressed in the `tool_calls` array format.
    fn assistant_message(&self, blocks: &[ContentBlock]) -> Value {
        let mut text_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<Value> = Vec::new();

        for block in blocks {
            match block {
                ContentBlock::Text { text } => {
                    text_parts.push(text.clone());
                }
                ContentBlock::ToolUse {
                    id, name, input, ..
                } => {
                    tool_calls.push(json!({
                        "id": id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": input.to_string()
                        }
                    }));
                }
                // CoT compat provider does not produce encrypted thinking blobs.
                ContentBlock::Thinking { .. } | ContentBlock::RedactedThinking { .. } => {}
            }
        }

        let content_text = text_parts.join("");

        if tool_calls.is_empty() {
            json!({
                "role": "assistant",
                "content": content_text
            })
        } else {
            json!({
                "role": "assistant",
                "content": if content_text.is_empty() { Value::Null } else { Value::String(content_text) },
                "tool_calls": tool_calls
            })
        }
    }

    fn tool_result_messages(&self, results: &[(String, String, bool)]) -> Vec<Value> {
        results
            .iter()
            .map(|(call_id, content, _is_error)| {
                json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": content
                })
            })
            .collect()
    }
}
