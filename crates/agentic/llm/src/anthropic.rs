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

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn stream(
        &self,
        system: &str,
        messages: &[Value],
        tools: &[ToolDef],
        thinking: &ThinkingConfig,
        response_schema: Option<&ResponseSchema>,
        max_tokens_override: Option<u32>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Chunk, LlmError>> + Send>>, LlmError> {
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

        let mut body = json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": messages,
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

        let mut req = self
            .client
            .post(&self.base_url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json");

        // Activate the extended-thinking beta so the API honours the
        // `thinking` body parameter and streams thinking blocks.
        if !matches!(effective_thinking, ThinkingConfig::Disabled) {
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
                            input_tokens = ev["message"]["usage"]["input_tokens"]
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
                                            .unwrap_or(Value::Null);
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
                            output_tokens = ev["usage"]["output_tokens"]
                                .as_u64()
                                .unwrap_or(0) as usize;
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
                            yield Ok(Chunk::Done(Usage { input_tokens, output_tokens, stop_reason }));
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
