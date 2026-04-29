use std::pin::Pin;

use async_stream::stream;
use async_trait::async_trait;
use futures_core::Stream;
use serde_json::{Value, json};

use agentic_core::tools::ToolDef;

use super::constants::*;
use super::sse::{ApiError, pop_sse_event, sse_data};
use super::{
    Chunk, ContentBlock, LlmError, LlmProvider, ResponseSchema, StopReason, ThinkingConfig,
    ToolCallChunk, Usage,
};

// ── AnthropicProvider ─────────────────────────────────────────────────────────

/// Anthropic Messages API provider (streaming).
///
/// Supports [`ThinkingConfig::Adaptive`] and [`ThinkingConfig::Manual`].
/// Encrypted thinking blobs (type + thinking + signature) are emitted as
/// [`Chunk::RawBlock`] and passed back verbatim between tool rounds.
pub struct AnthropicProvider {
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::Client,
}

impl AnthropicProvider {
    /// Create a provider using the given API key and model.
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            base_url: ANTHROPIC_API_URL.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Create a provider with a custom base URL (primarily for tests).
    #[cfg(test)]
    pub fn with_base_url(
        api_key: impl Into<String>,
        model: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            base_url: base_url.into(),
            client: reqwest::Client::new(),
        }
    }

    fn block_to_wire(block: &ContentBlock) -> Value {
        match block {
            ContentBlock::Thinking { provider_data } => provider_data.clone(),
            ContentBlock::RedactedThinking { provider_data } => provider_data.clone(),
            ContentBlock::Text { text } => json!({"type": "text", "text": text}),
            ContentBlock::ToolUse {
                id, name, input, ..
            } => json!({
                "type": "tool_use",
                "id": id,
                "name": name,
                "input": input
            }),
        }
    }
}

impl AnthropicProvider {
    /// Build the JSON request body for `/v1/messages`.
    ///
    /// Pure helper — no HTTP, no I/O — so unit tests can inspect the wire
    /// format directly.  Marks the system block and the last tool with
    /// `cache_control: ephemeral` so Anthropic caches the prefix.  When
    /// `system_date_suffix` is non-empty, it is appended as a second,
    /// uncached system content block so the time-varying date string does
    /// not invalidate the cached prefix.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn build_request_body(
        &self,
        system: &str,
        system_date_suffix: &str,
        messages: &[Value],
        tools: &[ToolDef],
        thinking: &ThinkingConfig,
        response_schema: Option<&ResponseSchema>,
        max_tokens_override: Option<u32>,
    ) -> Value {
        let mut tools_json: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                    "strict": t.strict
                })
            })
            .collect();

        // Choose max_tokens based on thinking config: Manual mode
        // requires max_tokens >= budget_tokens; Adaptive benefits from a
        // larger cap so the model can allocate freely.
        let max_tokens = max_tokens_override.unwrap_or_else(|| match thinking {
            ThinkingConfig::Manual { budget_tokens } => {
                std::cmp::max(THINKING_MAX_TOKENS, *budget_tokens + DEFAULT_MAX_TOKENS)
            }
            ThinkingConfig::Adaptive => THINKING_MAX_TOKENS,
            _ => DEFAULT_MAX_TOKENS,
        });

        // Mark the last message's last non-thinking content block with
        // cache_control so Round N reads the prior conversation history
        // (tool calls + tool results from Rounds 1..N-1) from cache instead
        // of re-paying for it.  Uses Anthropic's third breakpoint slot.
        let mut messages_owned: Vec<Value> = messages.to_vec();
        Self::mark_last_message_for_caching(&mut messages_owned);

        let mut body = json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "system": Self::build_system_blocks(system, system_date_suffix),
            "messages": messages_owned,
            "stream": true,
        });

        // Structured output: when no real tools, use output_config (constrained
        // decoding).  When real tools are present, inject a synthetic tool so the
        // model can call it when done — output_config + tools is unreliable on
        // smaller models (e.g. Haiku) which may return empty text.
        if let Some(schema) = response_schema {
            if tools.is_empty() {
                body["output_config"] = json!({
                    "format": {
                        "type": "json_schema",
                        "schema": schema.schema
                    }
                });
            } else {
                tools_json.push(json!({
                    "name": schema.name,
                    "description": "You MUST call this tool to return your final structured response. Do NOT embed JSON in your text — always use this tool.",
                    "input_schema": schema.schema,
                    "strict": true
                }));
            }
        }

        if !tools_json.is_empty() {
            // Mark the last tool with cache_control so the system + tools
            // prefix is cached.  Synthetic structured-response tools are
            // appended deterministically per state, so the array stays
            // byte-stable across rounds within one run_with_tools call.
            if let Some(last) = tools_json.last_mut() {
                last["cache_control"] = json!({"type": "ephemeral"});
            }
            body["tools"] = json!(tools_json);
        }

        // Map cross-provider thinking configs: OpenAI `Effort` → Anthropic
        // `Adaptive` so thinking is enabled regardless of which --thinking
        // flag the user passed.
        let effective_thinking = match thinking {
            ThinkingConfig::Effort(_) => &ThinkingConfig::Adaptive,
            other => other,
        };

        match effective_thinking {
            ThinkingConfig::Adaptive => {
                body["thinking"] = json!({"type": "adaptive"});
            }
            ThinkingConfig::Manual { budget_tokens } => {
                body["thinking"] = json!({"type": "enabled", "budget_tokens": budget_tokens});
            }
            ThinkingConfig::Disabled | ThinkingConfig::Effort(_) => {}
        }

        body
    }

    /// Mark the last non-thinking content block of the last message with
    /// `cache_control: ephemeral`.  No-op if `messages` is empty or the last
    /// message has no cacheable content.
    ///
    /// `cache_control` on a `thinking` or `redacted_thinking` block is
    /// rejected by the API, so the helper walks the last message's blocks
    /// from the end and marks the first non-thinking entry it finds.  In
    /// practice the last message is always a user/tool_result message
    /// (assistant tool_use turns are followed by their tool_result reply),
    /// so thinking blocks only appear earlier in the conversation — but the
    /// guard is cheap insurance against malformed history.
    ///
    /// String-valued `content` is lifted to a one-block array so the marker
    /// can be attached.
    fn mark_last_message_for_caching(messages: &mut [Value]) {
        let Some(last) = messages.last_mut() else {
            return;
        };
        match last.get_mut("content") {
            Some(Value::Array(blocks)) => {
                for block in blocks.iter_mut().rev() {
                    let ty = block.get("type").and_then(Value::as_str).unwrap_or("");
                    if ty == "thinking" || ty == "redacted_thinking" {
                        continue;
                    }
                    block["cache_control"] = json!({"type": "ephemeral"});
                    return;
                }
            }
            Some(Value::String(s)) => {
                let text = std::mem::take(s);
                last["content"] = json!([{
                    "type": "text",
                    "text": text,
                    "cache_control": {"type": "ephemeral"}
                }]);
            }
            _ => {}
        }
    }

    /// Construct the `system` field as a content-blocks array.
    ///
    /// - When `system` is non-empty, the static prefix gets a
    ///   `cache_control: ephemeral` breakpoint so Anthropic caches it.
    /// - When `system_date_suffix` is non-empty, it is emitted as a second
    ///   block *without* `cache_control` so daily date changes don't
    ///   invalidate the cached static prefix.
    fn build_system_blocks(system: &str, system_date_suffix: &str) -> Value {
        let mut blocks: Vec<Value> = Vec::new();
        if !system.is_empty() {
            blocks.push(json!({
                "type": "text",
                "text": system,
                "cache_control": {"type": "ephemeral"}
            }));
        }
        if !system_date_suffix.is_empty() {
            blocks.push(json!({
                "type": "text",
                "text": system_date_suffix
            }));
        }
        Value::Array(blocks)
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn stream(
        &self,
        system: &str,
        system_date_suffix: &str,
        messages: &[Value],
        tools: &[ToolDef],
        thinking: &ThinkingConfig,
        response_schema: Option<&ResponseSchema>,
        max_tokens_override: Option<u32>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Chunk, LlmError>> + Send>>, LlmError> {
        let body = self.build_request_body(
            system,
            system_date_suffix,
            messages,
            tools,
            thinking,
            response_schema,
            max_tokens_override,
        );

        let mut req = self
            .client
            .post(&self.base_url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json");

        // Activate the extended-thinking beta whenever any thinking mode is
        // requested (including `Effort`, which `build_request_body` maps to
        // `Adaptive` in the body).  The mapping is centralised there; here we
        // only need to know "is thinking on at all?".
        if !matches!(thinking, ThinkingConfig::Disabled) {
            req = req.header("anthropic-beta", ANTHROPIC_THINKING_BETA);
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

        let s = stream! {
            use tokio_stream::StreamExt as _;

            let mut sse_buf = String::new();
            // Current open content block
            let mut block_type: Option<String> = None;
            // Thinking accumulator
            let mut thinking_text = String::new();
            let mut thinking_sig = String::new();
            // Tool-use accumulator
            let mut tool_id = String::new();
            let mut tool_name = String::new();
            let mut tool_args = String::new();
            // Usage
            let mut input_tokens: usize = 0;
            let mut output_tokens: usize = 0;
            let mut cache_creation_input_tokens: usize = 0;
            let mut cache_read_input_tokens: usize = 0;
            let mut stop_reason = StopReason::EndTurn;

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
                        break 'outer;
                    }

                    let ev: Value = match serde_json::from_str(data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    match ev["type"].as_str().unwrap_or("") {
                        "message_start" => {
                            let usage = &ev["message"]["usage"];
                            input_tokens = usage["input_tokens"].as_u64().unwrap_or(0) as usize;
                            // Cache token fields are only present when prompt
                            // caching engaged on this call.  Treat absence as 0.
                            cache_creation_input_tokens = usage
                                ["cache_creation_input_tokens"]
                                .as_u64()
                                .unwrap_or(0) as usize;
                            cache_read_input_tokens = usage
                                ["cache_read_input_tokens"]
                                .as_u64()
                                .unwrap_or(0) as usize;
                        }

                        "content_block_start" => {
                            let cb = &ev["content_block"];
                            let btype = cb["type"].as_str().unwrap_or("").to_string();
                            match btype.as_str() {
                                "thinking" => {
                                    thinking_text.clear();
                                    thinking_sig.clear();
                                    // Empty initial chunk signals ThinkingStart to the consumer.
                                    yield Ok(Chunk::ThinkingSummary(String::new()));
                                }
                                "text" => {
                                    // Empty initial chunk signals start of text block.
                                    yield Ok(Chunk::Text(String::new()));
                                }
                                "tool_use" => {
                                    tool_id = cb["id"].as_str().unwrap_or("").to_string();
                                    tool_name = cb["name"].as_str().unwrap_or("").to_string();
                                    tool_args.clear();
                                }
                                _ => {}
                            }
                            block_type = Some(btype);
                        }

                        "content_block_delta" => {
                            let delta = &ev["delta"];
                            match delta["type"].as_str().unwrap_or("") {
                                "thinking_delta" => {
                                    let t = delta["thinking"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string();
                                    thinking_text.push_str(&t);
                                    yield Ok(Chunk::ThinkingSummary(t));
                                }
                                "signature_delta" => {
                                    thinking_sig.push_str(
                                        delta["signature"].as_str().unwrap_or(""),
                                    );
                                }
                                "text_delta" => {
                                    let t =
                                        delta["text"].as_str().unwrap_or("").to_string();
                                    yield Ok(Chunk::Text(t));
                                }
                                "input_json_delta" => {
                                    tool_args.push_str(
                                        delta["partial_json"].as_str().unwrap_or(""),
                                    );
                                }
                                _ => {}
                            }
                        }

                        "content_block_stop" => {
                            match block_type.as_deref() {
                                Some("thinking") => {
                                    let mut obj = serde_json::Map::new();
                                    obj.insert("type".into(), json!("thinking"));
                                    obj.insert(
                                        "thinking".into(),
                                        json!(thinking_text.clone()),
                                    );
                                    obj.insert(
                                        "signature".into(),
                                        json!(thinking_sig.clone()),
                                    );
                                    yield Ok(Chunk::RawBlock(ContentBlock::Thinking {
                                        provider_data: Value::Object(obj),
                                    }));
                                }
                                Some("tool_use") => {
                                    let input: Value =
                                        serde_json::from_str(&tool_args)
                                            .unwrap_or_else(|_| json!({}));
                                    yield Ok(Chunk::ToolCall(ToolCallChunk {
                                        id: tool_id.clone(),
                                        name: tool_name.clone(),
                                        input,
                                        provider_data: None,
                                    }));
                                }
                                _ => {}
                            }
                            block_type = None;
                        }

                        "message_delta" => {
                            let usage = &ev["usage"];
                            output_tokens =
                                usage["output_tokens"].as_u64().unwrap_or(0) as usize;
                            // Anthropic occasionally re-reports cache tokens
                            // here; take max so a later 0 doesn't clobber a
                            // value seen at message_start.
                            if let Some(v) = usage["cache_creation_input_tokens"].as_u64()
                            {
                                cache_creation_input_tokens =
                                    cache_creation_input_tokens.max(v as usize);
                            }
                            if let Some(v) = usage["cache_read_input_tokens"].as_u64() {
                                cache_read_input_tokens =
                                    cache_read_input_tokens.max(v as usize);
                            }
                            // Parse stop_reason: "end_turn", "max_tokens", or "tool_use".
                            if let Some(sr) = ev["delta"]["stop_reason"].as_str() {
                                stop_reason = match sr {
                                    "max_tokens" => StopReason::MaxTokens,
                                    "tool_use" => StopReason::ToolUse,
                                    _ => StopReason::EndTurn,
                                };
                            }
                        }

                        "message_stop" => {
                            yield Ok(Chunk::Done(Usage {
                                input_tokens,
                                output_tokens,
                                cache_creation_input_tokens,
                                cache_read_input_tokens,
                                stop_reason,
                            }));
                            break 'outer;
                        }

                        _ => {}
                    }
                }
            }
        };

        Ok(Box::pin(s))
    }

    fn assistant_message(&self, blocks: &[ContentBlock]) -> Value {
        let content: Vec<Value> = blocks.iter().map(Self::block_to_wire).collect();
        json!({"role": "assistant", "content": content})
    }

    fn tool_result_messages(&self, results: &[(String, String, bool)]) -> Vec<Value> {
        let result_blocks: Vec<Value> = results
            .iter()
            .map(|(id, content, is_error)| {
                json!({
                    "type": "tool_result",
                    "tool_use_id": id,
                    "content": content,
                    "is_error": is_error
                })
            })
            .collect();
        vec![json!({"role": "user", "content": result_blocks})]
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
