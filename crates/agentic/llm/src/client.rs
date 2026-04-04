use std::sync::Arc;
use std::time::Instant;

use serde_json::{Value, json};

use agentic_core::events::{CoreEvent, DomainEvents, Event, EventStream};
use agentic_core::tools::ToolDef;

use super::constants::{DEFAULT_MAX_TOKENS, DEFAULT_MODEL, THINKING_MAX_TOKENS};
use super::{
    AnthropicProvider, Chunk, ContentBlock, InitialMessages, LlmError, LlmOutput, LlmProvider,
    OpenAiCompatProvider, StopReason, ThinkingConfig, ToolCallChunk, ToolLoopConfig, Usage,
};

// ── LlmClient ─────────────────────────────────────────────────────────────────

/// High-level client that wraps an [`LlmProvider`] with tool-use loop logic
/// and event emission.
///
/// Construct with [`LlmClient::new`] (defaults to [`AnthropicProvider`]) or
/// supply a custom provider via [`LlmClient::with_provider`].
///
/// # Example
///
/// ```no_run
/// # use agentic_llm::LlmClient;
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let client = LlmClient::new(std::env::var("ANTHROPIC_API_KEY")?);
/// let answer = client.complete("You are helpful.", "What is 2 + 2?").await?;
/// println!("{answer}");
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct LlmClient {
    provider: Arc<dyn LlmProvider>,
}

impl LlmClient {
    /// Create a client backed by [`AnthropicProvider`] with the default model.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            provider: Arc::new(AnthropicProvider::new(api_key, DEFAULT_MODEL)),
        }
    }

    /// Create a client backed by [`AnthropicProvider`] with a custom model.
    pub fn with_model(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: Arc::new(AnthropicProvider::new(api_key, model)),
        }
    }

    /// Create a client backed by [`OpenAiCompatProvider`] (Chat Completions API).
    ///
    /// Use this for Ollama, vLLM, LM Studio, and any other OpenAI-compatible
    /// backend.  `base_url` should be the API root, e.g.
    /// `"http://localhost:11434/v1"`.
    pub fn with_openai_compat(
        api_key: impl Into<String>,
        model: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            provider: Arc::new(OpenAiCompatProvider::new(api_key, model, base_url)),
        }
    }

    /// Create a client backed by a fully custom provider.
    pub fn with_provider(provider: impl LlmProvider + 'static) -> Self {
        Self {
            provider: Arc::new(provider),
        }
    }

    /// Build the message history for resuming after an `ask_user` suspension.
    ///
    /// `prior_messages` is the full provider-native message history returned
    /// inside [`LlmError::Suspended`] — it already contains the user turn,
    /// every prior tool round, and the assistant turn with the `ask_user` call.
    /// This function appends the tool result (the user's answer) and returns
    /// the complete history ready for [`InitialMessages::Messages`].
    ///
    /// Passing `prior_messages` ensures the LLM sees all tool calls it made
    /// before suspending, not just the ask_user exchange.
    pub fn build_resume_messages(
        &self,
        prior_messages: &[Value],
        question: &str,
        suggestions: &[String],
        answer: &str,
    ) -> Vec<Value> {
        // The ask_user tool call in the last assistant turn uses an id
        // assigned by the model.  Extract it so the tool_result we append
        // references the correct id.  The message format differs per
        // provider, so we check all three shapes:
        //
        //  1. Anthropic:           role:"assistant" → content[{type:"tool_use", name, id}]
        //  2. OpenAI Responses:    flat item {type:"function_call", name, call_id}
        //  3. OpenAI Chat Compl.:  role:"assistant" → tool_calls[{id, function:{name}}]
        tracing::trace!(
            prior_messages_count = prior_messages.len(),
            "build_resume_messages called"
        );

        let tool_use_id =
            find_ask_user_id(prior_messages).unwrap_or_else(|| "ask_user_0".to_string());

        let _ = (question, suggestions); // already encoded in prior_messages
        let mut msgs = prior_messages.to_vec();
        let new_results =
            self.provider
                .tool_result_messages(&[(tool_use_id, answer.to_string(), false)]);

        // When `ask_user` was in a batch with other tools, the tool loop
        // flushes results for already-executed tools before suspending.
        // For Anthropic, those results are a single user message with
        // `tool_result` blocks.  We must merge the ask_user result into
        // that existing message rather than appending a new user message
        // (Anthropic requires ALL tool_results for a batch in one message).
        if let Some(last) = msgs.last_mut()
            && last["role"].as_str() == Some("user")
            && let Some(content) = last["content"].as_array()
        {
            let has_tool_results = content
                .iter()
                .any(|b| b["type"].as_str() == Some("tool_result"));
            if has_tool_results {
                // Merge: extract tool_result blocks from new_results
                // and append them to the existing user message content.
                let mut merged_content = content.clone();
                for r in &new_results {
                    if let Some(blocks) = r["content"].as_array() {
                        merged_content.extend(blocks.iter().cloned());
                    }
                }
                last["content"] = Value::Array(merged_content);
                return msgs;
            }
        }

        // No partial results to merge — just append.
        msgs.extend(new_results);
        msgs
    }

    /// Build the message history for resuming after a [`LlmError::MaxTokensReached`]
    /// or [`LlmError::MaxToolRoundsReached`] suspension.
    ///
    /// `prior_messages` is the full provider-native message history stored at
    /// the suspension point.  This function appends a user "please continue"
    /// message so the LLM resumes from where it left off.
    ///
    /// For `MaxTokensReached`: `prior_messages` already contains the truncated
    /// assistant turn; pass back with a doubled `max_tokens_override`.
    ///
    /// For `MaxToolRoundsReached`: `prior_messages` is the history up to the
    /// last tool results (before the model's unanswered tool request); pass
    /// back with an increased `max_tool_rounds`.
    pub fn build_continue_messages(prior_messages: &[Value]) -> Vec<Value> {
        let mut msgs = prior_messages.to_vec();
        msgs.push(json!({"role": "user", "content": "Please continue."}));
        msgs
    }

    /// Send a single-turn completion and return the assistant's text response.
    pub async fn complete(&self, system: &str, user: &str) -> Result<String, LlmError> {
        let (text, _) = self.complete_with_usage(system, user).await?;
        Ok(text)
    }

    /// Like [`complete`] but also returns the [`Usage`] reported by the API.
    ///
    /// [`complete`]: LlmClient::complete
    #[tracing::instrument(
        skip_all,
        fields(
            otel.name = "llm.call",
            oxy.span_type = "llm",
            gen_ai.request.model = %self.provider.model_name(),
        )
    )]
    pub async fn complete_with_usage(
        &self,
        system: &str,
        user: &str,
    ) -> Result<(String, Usage), LlmError> {
        let messages = vec![json!({"role": "user", "content": user})];
        let s = self
            .provider
            .stream(
                system,
                &messages,
                &[],
                &ThinkingConfig::Disabled,
                None,
                None,
            )
            .await?;

        let mut text = String::new();
        let mut usage = Usage::default();

        let mut s = std::pin::pin!(s);
        while let Some(chunk) = {
            use tokio_stream::StreamExt as _;
            s.next().await
        } {
            match chunk? {
                Chunk::Text(t) => text.push_str(&t),
                Chunk::Done(u) => usage = u,
                _ => {}
            }
        }

        if text.is_empty() {
            return Err(LlmError::Parse("no text content in response".into()));
        }

        tracing::info!(
            name: "llm.usage",
            is_visible = true,
            prompt_tokens = usage.input_tokens as i64,
            completion_tokens = usage.output_tokens as i64,
            total_tokens = (usage.input_tokens + usage.output_tokens) as i64,
            model = %self.provider.model_name(),
            stop_reason = %format!("{:?}", usage.stop_reason),
        );

        Ok((text, usage))
    }

    /// Run the LLM with an optional tool-use loop, emitting granular streaming
    /// events.
    ///
    /// # Event sequence
    ///
    /// ```text
    /// LlmStart                          ← once per state invocation
    ///   ThinkingStart / ThinkingToken* / ThinkingEnd   ← per thinking block
    ///   LlmToken*                        ← per text token
    ///   ToolCall / ToolResult            ← per tool round
    ///   … (repeat per tool round)
    /// LlmEnd                            ← once, on normal exit
    /// ```
    ///
    /// `LlmStart` / `LlmEnd` appear **exactly once** regardless of how many
    /// tool rounds occur.  `ThinkingStart` / `ThinkingEnd` pairs may appear
    /// multiple times (once per thinking block, including interleaved thinking
    /// between tool rounds).
    ///
    /// # Thinking blobs
    ///
    /// Encrypted thinking blobs are preserved within the tool-use loop and
    /// returned in [`LlmOutput::raw_content_blocks`].  The caller (orchestrator)
    /// **must** discard them on every FSM state transition.
    ///
    /// # Tool executor
    ///
    /// `tool_executor` is an **async** closure.  Catalog-only tools wrap their
    /// synchronous result with `Box::pin(async move { sync_result })`.
    /// Connector-backed tools (e.g. `sample_column`, `execute_preview`) call
    /// `connector.execute_query(...).await` directly inside the box.
    pub async fn run_with_tools<Ev, F>(
        &self,
        system: &str,
        initial: impl Into<InitialMessages>,
        tools: &[ToolDef],
        mut tool_executor: F,
        events: &Option<EventStream<Ev>>,
        config: ToolLoopConfig,
    ) -> Result<LlmOutput, LlmError>
    where
        Ev: DomainEvents,
        F: FnMut(
            String,
            Value,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<Value, agentic_core::tools::ToolError>>
                    + Send,
            >,
        >,
    {
        let (mut messages, prompt_tokens) = match initial.into() {
            InitialMessages::User(user) => {
                let tokens = system.len() / 4 + user.len() / 4;
                (vec![json!({"role": "user", "content": user})], tokens)
            }
            InitialMessages::Messages(msgs) => {
                let tokens = system.len() / 4;
                (msgs, tokens)
            }
        };
        let start_time = Instant::now();
        let ssi = config.sub_spec_index;

        emit_core(
            events,
            CoreEvent::LlmStart {
                state: config.state.clone(),
                prompt_tokens,
                sub_spec_index: ssi,
            },
        )
        .await;

        let mut rounds: u32 = 0;
        let mut effective_max_tokens = config.max_tokens_override;
        // Collects text produced in rounds that also contain tool calls so that
        // all narrative output is preserved, not just the final round's text.
        let mut accumulated_text = String::new();
        // Accumulates every tool call made across all rounds.
        let mut all_tool_calls: Vec<(String, serde_json::Value)> = Vec::new();

        loop {
            let round_start = Instant::now();

            // Create an OTel span for this LLM inference round.  The span
            // covers only the API call + stream consumption; it is explicitly
            // dropped before tool execution so its duration reflects pure
            // inference time.
            let llm_span = tracing::info_span!(
                "llm_round",
                otel.name = "llm.call",
                oxy.span_type = "llm",
                gen_ai.request.model = %self.provider.model_name(),
                llm.state = %config.state,
                llm.round = rounds,
            );
            let _llm_guard = llm_span.enter();

            let s = self
                .provider
                .stream(
                    system,
                    &messages,
                    tools,
                    &config.thinking,
                    config.response_schema.as_ref(),
                    effective_max_tokens,
                )
                .await?;

            let mut text = String::new();
            let mut thinking_summary = String::new();
            let mut tool_calls: Vec<ToolCallChunk> = Vec::new();
            let mut raw_blocks: Vec<ContentBlock> = Vec::new();
            // Track the original stream order of reasoning items and tool
            // calls so we can reconstruct the assistant turn with correct
            // interleaving.  OpenAI requires each reasoning item to be
            // immediately followed by its output item (function_call /
            // message); re-ordering breaks that invariant.
            let mut ordered_blocks: Vec<ContentBlock> = Vec::new();
            let mut in_thinking = false;
            let mut usage = Usage::default();
            let mut stream_err: Option<LlmError> = None;

            let mut s = std::pin::pin!(s);
            loop {
                use tokio_stream::StreamExt as _;
                let Some(chunk) = s.next().await else { break };
                match chunk {
                    Err(e) => {
                        stream_err = Some(e);
                        break;
                    }
                    Ok(chunk) => match chunk {
                        Chunk::ThinkingSummary(t) => {
                            if !in_thinking {
                                in_thinking = true;
                                emit_core(
                                    events,
                                    CoreEvent::ThinkingStart {
                                        state: config.state.clone(),
                                        sub_spec_index: ssi,
                                    },
                                )
                                .await;
                            }
                            thinking_summary.push_str(&t);
                            if !t.is_empty() {
                                emit_core(
                                    events,
                                    CoreEvent::ThinkingToken {
                                        token: t,
                                        sub_spec_index: ssi,
                                    },
                                )
                                .await;
                            }
                        }
                        Chunk::Text(t) => {
                            if in_thinking {
                                in_thinking = false;
                                emit_core(
                                    events,
                                    CoreEvent::ThinkingEnd {
                                        state: config.state.clone(),
                                        sub_spec_index: ssi,
                                    },
                                )
                                .await;
                            }
                            text.push_str(&t);
                            // Suppress token streaming when the LLM is
                            // producing a structured JSON response via
                            // response_schema — the raw JSON is not
                            // user-friendly and the meaningful content is
                            // already captured in domain events
                            // (TriageCompleted, IntentClarified, etc.).
                            if !t.is_empty() && config.response_schema.is_none() {
                                emit_core(
                                    events,
                                    CoreEvent::LlmToken {
                                        token: t,
                                        sub_spec_index: ssi,
                                    },
                                )
                                .await;
                            }
                        }
                        Chunk::ToolCall(tc) => {
                            ordered_blocks.push(ContentBlock::ToolUse {
                                id: tc.id.clone(),
                                name: tc.name.clone(),
                                input: tc.input.clone(),
                                provider_data: tc.provider_data.clone(),
                            });
                            tool_calls.push(tc);
                        }
                        Chunk::RawBlock(block) => {
                            ordered_blocks.push(block.clone());
                            raw_blocks.push(block);
                        }
                        Chunk::Done(u) => {
                            usage = u;
                        }
                    },
                }
            }

            // Close any open thinking block (stream ended mid-thinking).
            if in_thinking {
                emit_core(
                    events,
                    CoreEvent::ThinkingEnd {
                        state: config.state.clone(),
                        sub_spec_index: ssi,
                    },
                )
                .await;
            }

            // Capture the LLM inference time for this round (excludes tool execution).
            let round_llm_ms = round_start.elapsed().as_millis() as u64;

            // Record token usage as a visible span event on the LLM span so
            // it appears in the ClickHouse trace detail view.
            tracing::info!(
                name: "llm.usage",
                is_visible = true,
                prompt_tokens = usage.input_tokens as i64,
                completion_tokens = usage.output_tokens as i64,
                total_tokens = (usage.input_tokens + usage.output_tokens) as i64,
                model = %self.provider.model_name(),
                duration_ms = round_llm_ms,
                stop_reason = %format!("{:?}", usage.stop_reason),
            );

            // Record the LLM output (text + tool calls) as a visible event.
            {
                let tool_names: Vec<&str> = tool_calls.iter().map(|tc| tc.name.as_str()).collect();
                let output_preview = if text.len() > 2000 {
                    format!("{}… ({} chars)", &text[..2000], text.len())
                } else {
                    text.clone()
                };
                tracing::info!(
                    name: "llm.output",
                    is_visible = true,
                    text = %output_preview,
                    tool_calls = %serde_json::to_string(&tool_names).unwrap_or_default(),
                );
            }

            // Close the LLM span before tool execution so its duration
            // reflects only inference time, not tool latency.
            drop(_llm_guard);
            drop(llm_span);

            // Propagate stream errors — still emit LlmEnd so the consumer
            // always sees a balanced Start/End pair.
            if let Some(e) = stream_err {
                emit_core(
                    events,
                    CoreEvent::LlmEnd {
                        state: config.state.clone(),
                        output_tokens: usage.output_tokens,
                        duration_ms: start_time.elapsed().as_millis() as u64,
                        model: self.provider.model_name().to_string(),
                        sub_spec_index: ssi,
                    },
                )
                .await;
                return Err(e);
            }

            // Check if any tool call is the structured-response tool.
            // When found, treat it as the final output rather than executing it.
            if let Some(ref schema) = config.response_schema
                && let Some(schema_tc) = tool_calls.iter().find(|tc| tc.name == schema.name)
            {
                let structured = schema_tc.input.clone();
                let text_json =
                    serde_json::to_string(&structured).unwrap_or_else(|_| "{}".to_string());
                emit_core(
                    events,
                    CoreEvent::LlmEnd {
                        state: config.state.clone(),
                        output_tokens: usage.output_tokens,
                        duration_ms: start_time.elapsed().as_millis() as u64,
                        model: self.provider.model_name().to_string(),
                        sub_spec_index: ssi,
                    },
                )
                .await;
                return Ok(LlmOutput {
                    text: text_json,
                    thinking_summary: if thinking_summary.is_empty() {
                        None
                    } else {
                        Some(thinking_summary)
                    },
                    raw_content_blocks: raw_blocks,
                    structured_response: Some(structured),
                    tool_calls: all_tool_calls,
                });
            }

            // No tool calls — final response.
            if tool_calls.is_empty() {
                // ── Empty-response guard ───────────────────────────────────────
                // The model produced thinking/reasoning but no text content.
                // Two sub-cases:
                //
                // 1. `MaxTokens` — the thinking phase consumed the entire
                //    token budget.  Retry with a doubled limit.
                // 2. Any other stop reason (e.g. `EndTurn`) — the model
                //    chose to stop without emitting text (common with
                //    OpenAI reasoning + structured output).  Retry once;
                //    if that also fails, return EmptyResponse.
                if text.trim().is_empty() && !thinking_summary.is_empty() {
                    if rounds < config.max_tool_rounds {
                        // The model produced thinking but no text.  Instead of
                        // discarding the thinking and retrying from scratch,
                        // preserve it by appending the assistant turn + a
                        // "Continue" user message.  This avoids wasting the
                        // thinking budget and nudges the model to emit text.
                        //
                        // Anthropic rejects assistant messages whose final
                        // block is a `thinking` block, so we append an empty
                        // text block as a sentinel to satisfy the constraint.
                        let mut assistant_blocks: Vec<ContentBlock> = Vec::new();
                        assistant_blocks.extend(ordered_blocks);
                        assistant_blocks.push(ContentBlock::Text {
                            text: String::new(),
                        });
                        messages.push(self.provider.assistant_message(&assistant_blocks));
                        messages.push(json!({"role": "user", "content": "Continue."}));

                        if usage.stop_reason == StopReason::MaxTokens {
                            let current = effective_max_tokens.unwrap_or(THINKING_MAX_TOKENS);
                            effective_max_tokens = Some(current.saturating_mul(2));
                        }
                        rounds += 1;
                        continue; // re-enter the outer provider.stream() loop
                    }
                    // Exhausted retries — return an error.
                    emit_core(
                        events,
                        CoreEvent::LlmEnd {
                            state: config.state.clone(),
                            output_tokens: usage.output_tokens,
                            duration_ms: start_time.elapsed().as_millis() as u64,
                            model: self.provider.model_name().to_string(),
                            sub_spec_index: ssi,
                        },
                    )
                    .await;
                    let reason = if usage.stop_reason == StopReason::MaxTokens {
                        "model hit max_tokens during thinking; retry also truncated"
                    } else {
                        "model produced reasoning but no text output; retries exhausted"
                    };
                    return Err(LlmError::EmptyResponse {
                        reason: reason.into(),
                    });
                }

                // Merge text from all prior tool-call rounds with this final round.
                // Insert a newline separator between accumulated and final-round
                // text when neither side is empty and there is no trailing newline.
                let text = if accumulated_text.is_empty() {
                    text
                } else if text.is_empty() {
                    accumulated_text
                } else {
                    if !accumulated_text.ends_with('\n') {
                        accumulated_text.push('\n');
                    }
                    accumulated_text.push_str(&text);
                    accumulated_text
                };

                // Text was truncated by the token limit — suspend so the caller
                // can ask the user whether to double the budget and continue.
                // (The thinking-only MaxTokens case is handled earlier above.)
                if usage.stop_reason == StopReason::MaxTokens {
                    // Append the (partial) assistant turn so the resume context
                    // includes what was generated before the cutoff.
                    let mut assistant_blocks = vec![ContentBlock::Text { text: text.clone() }];
                    assistant_blocks.extend(ordered_blocks);
                    messages.push(self.provider.assistant_message(&assistant_blocks));

                    emit_core(
                        events,
                        CoreEvent::LlmEnd {
                            state: config.state.clone(),
                            output_tokens: usage.output_tokens,
                            duration_ms: start_time.elapsed().as_millis() as u64,
                            model: self.provider.model_name().to_string(),
                            sub_spec_index: ssi,
                        },
                    )
                    .await;
                    return Err(LlmError::MaxTokensReached {
                        partial_text: text,
                        current_max_tokens: effective_max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
                        prior_messages: messages,
                    });
                }

                emit_core(
                    events,
                    CoreEvent::LlmEnd {
                        state: config.state.clone(),
                        output_tokens: usage.output_tokens,
                        duration_ms: start_time.elapsed().as_millis() as u64,
                        model: self.provider.model_name().to_string(),
                        sub_spec_index: ssi,
                    },
                )
                .await;

                // When a response_schema is configured and the model returned
                // plain JSON text (OpenAI response_format path), parse it into
                // structured_response so callers get the same shape as the
                // tool-call path.
                let structured_response = if config.response_schema.is_some() {
                    serde_json::from_str::<Value>(&text).ok()
                } else {
                    None
                };

                return Ok(LlmOutput {
                    text,
                    thinking_summary: if thinking_summary.is_empty() {
                        None
                    } else {
                        Some(thinking_summary)
                    },
                    raw_content_blocks: raw_blocks,
                    structured_response,
                    tool_calls: all_tool_calls,
                });
            }

            // Round-limit check before processing tool calls.
            if rounds >= config.max_tool_rounds {
                emit_core(
                    events,
                    CoreEvent::LlmEnd {
                        state: config.state.clone(),
                        output_tokens: usage.output_tokens,
                        duration_ms: start_time.elapsed().as_millis() as u64,
                        model: self.provider.model_name().to_string(),
                        sub_spec_index: ssi,
                    },
                )
                .await;
                return Err(LlmError::MaxToolRoundsReached {
                    rounds,
                    prior_messages: messages.clone(),
                });
            }

            // Append the full assistant turn in **stream order**.
            //
            // `ordered_blocks` preserves the exact interleaving of reasoning
            // items and function-call items as produced by the model.  OpenAI
            // requires each reasoning item to be immediately followed by its
            // output item; reordering (e.g. all reasoning first, then all
            // tool calls) breaks that invariant and causes:
            //   "Item '…' of type 'reasoning' was provided without its
            //    required following item."
            //
            // Text (accumulated from stream deltas) is prepended **before**
            // the interleaved reasoning/function_call items.  Anthropic
            // requires text content blocks to precede tool_use blocks in
            // the assistant message — placing text at the end causes:
            //   "tool_use ids were found without tool_result blocks
            //    immediately after"
            // OpenAI's assistant_message() independently flushes text parts
            // before each reasoning/function_call item, so the input order
            // of text relative to those items is handled correctly either way.
            let mut assistant_blocks = Vec::new();
            if !text.is_empty() {
                assistant_blocks.push(ContentBlock::Text { text: text.clone() });
            }
            assistant_blocks.extend(ordered_blocks);
            messages.push(self.provider.assistant_message(&assistant_blocks));

            // Execute tools and collect results.
            let mut tool_results: Vec<(String, String, bool)> = Vec::new();
            for tc in &tool_calls {
                all_tool_calls.push((tc.name.clone(), tc.input.clone()));
                emit_core(
                    events,
                    CoreEvent::ToolCall {
                        name: tc.name.clone(),
                        input: tc.input.to_string(),
                        llm_duration_ms: round_llm_ms,
                        sub_spec_index: ssi,
                    },
                )
                .await;

                let t_tool = Instant::now();
                let exec_result = tool_executor(tc.name.clone(), tc.input.clone()).await;
                let tool_ms = t_tool.elapsed().as_millis() as u64;

                // Early-return for ask_user suspension.
                //
                // Before returning, flush any tool results collected so far
                // (from tools executed earlier in this batch).  Without this,
                // prior_messages would contain the assistant turn with ALL
                // function_calls but only the ask_user output — causing
                // "No tool output found for function call" from OpenAI.
                if let Err(agentic_core::tools::ToolError::Suspended {
                    ref prompt,
                    ref suggestions,
                }) = exec_result
                {
                    // Flush already-collected results for tools executed
                    // before ask_user in this batch (if any).
                    if !tool_results.is_empty() {
                        for msg in self.provider.tool_result_messages(&tool_results) {
                            messages.push(msg);
                        }
                    }

                    emit_core(
                        events,
                        CoreEvent::LlmEnd {
                            state: config.state.clone(),
                            output_tokens: 0,
                            duration_ms: start_time.elapsed().as_millis() as u64,
                            model: self.provider.model_name().to_string(),
                            sub_spec_index: ssi,
                        },
                    )
                    .await;
                    return Err(LlmError::Suspended {
                        prompt: prompt.clone(),
                        suggestions: suggestions.clone(),
                        prior_messages: messages.clone(),
                    });
                }

                let (content_str, is_error) = match exec_result {
                    Ok(v) => (v.to_string(), false),
                    Err(e) => (e.to_string(), true),
                };

                emit_core(
                    events,
                    CoreEvent::ToolResult {
                        name: tc.name.clone(),
                        output: content_str.clone(),
                        duration_ms: tool_ms,
                        sub_spec_index: ssi,
                    },
                )
                .await;

                tool_results.push((tc.id.clone(), content_str, is_error));
            }

            for msg in self.provider.tool_result_messages(&tool_results) {
                messages.push(msg);
            }

            // Save this round's text before the next iteration resets it.
            // Insert a newline separator when the running text doesn't already
            // end with one so that consecutive rounds read as distinct chunks.
            if !text.is_empty() {
                if !accumulated_text.is_empty() && !accumulated_text.ends_with('\n') {
                    accumulated_text.push('\n');
                }
                accumulated_text.push_str(&text);
            }
            rounds += 1;
            // LlmStart is NOT re-emitted; this is the same logical invocation.
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

pub(super) async fn emit_core<Ev: DomainEvents>(tx: &Option<EventStream<Ev>>, event: CoreEvent) {
    if let Some(tx) = tx {
        let _ = tx.send(Event::Core(event)).await;
    }
}

/// Extract the tool-call ID of the last `ask_user` invocation from a
/// provider-native message history.  Supports all three provider formats:
///
///  - **Anthropic**: `{role:"assistant", content:[{type:"tool_use", name:"ask_user", id}]}`
///  - **OpenAI Responses API**: items stored as a `Value::Array` element
///    containing `{type:"function_call", name:"ask_user", call_id}` items
///  - **OpenAI Chat Completions**: `{role:"assistant", tool_calls:[{id, function:{name:"ask_user"}}]}`
pub(crate) fn find_ask_user_id(messages: &[Value]) -> Option<String> {
    for m in messages.iter().rev() {
        // 1. Anthropic: role:"assistant", content array with tool_use blocks
        if m["role"].as_str() == Some("assistant") {
            if let Some(blocks) = m["content"].as_array() {
                for b in blocks.iter().rev() {
                    if b["type"].as_str() == Some("tool_use")
                        && b["name"].as_str() == Some("ask_user")
                    {
                        return b["id"].as_str().map(String::from);
                    }
                }
            }
            // 3. OpenAI Chat Completions: tool_calls array
            if let Some(tool_calls) = m["tool_calls"].as_array() {
                for tc in tool_calls.iter().rev() {
                    if tc["function"]["name"].as_str() == Some("ask_user") {
                        return tc["id"].as_str().map(String::from);
                    }
                }
            }
        }

        // 2a. OpenAI Responses API: assistant_message() returns
        //     Value::Array([...items...]) which gets pushed as one element
        //     in the messages vec.  Look inside for function_call items.
        if let Some(items) = m.as_array() {
            for item in items.iter().rev() {
                if item["type"].as_str() == Some("function_call")
                    && item["name"].as_str() == Some("ask_user")
                {
                    return item["call_id"].as_str().map(String::from);
                }
            }
        }

        // 2b. OpenAI Responses API: flat function_call item (in case
        //     the array was already flattened into messages).
        if m["type"].as_str() == Some("function_call") && m["name"].as_str() == Some("ask_user") {
            return m["call_id"].as_str().map(String::from);
        }
    }
    None
}

/// Try to extract the first top-level JSON object from `text` that may be
/// surrounded by prose or markdown fences.  Returns `None` if no valid
/// object is found.
fn extract_json_object(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    // Walk from the end backwards to find the matching closing brace.
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&text[start..=end]).ok()
}
