use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;
use serde_json::Value;

use agentic_core::tools::ToolDef;

use super::{Chunk, ContentBlock, LlmError, ResponseSchema, ThinkingConfig};

// ── LlmProvider trait ─────────────────────────────────────────────────────────

/// Abstraction over LLM providers (Anthropic, OpenAI, …).
///
/// Implement this to plug a new model family into [`LlmClient`].
///
/// [`stream`] returns a pinned [`Stream`] of [`Chunk`] items, consuming the
/// provider's SSE response.  The message-formatting helpers are called by
/// [`LlmClient::run_with_tools`] to build the conversation history.
///
/// [`stream`]: LlmProvider::stream
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Open a streaming connection to the LLM and return a [`Chunk`] stream.
    ///
    /// `system` is the system prompt.  `messages` is the full conversation
    /// history in provider-native JSON format — including any thinking blocks
    /// from previous turns that must be preserved verbatim.
    ///
    /// When `response_schema` is `Some`, the provider injects constrained
    /// decoding so the final reply is guaranteed to match the schema:
    /// - Anthropic: a synthetic tool is appended and `tool_choice` is set.
    /// - OpenAI: `response_format` with `json_schema`/`strict` is added.
    ///
    /// `max_tokens_override`, when `Some`, takes precedence over the
    /// provider's default (`ThinkingConfig`-derived) `max_tokens` value.
    ///
    /// `system_date_suffix`, when non-empty, is a dynamic addendum to the
    /// system prompt that must NOT be part of the prompt-cache prefix.
    /// Anthropic emits it as a separate uncached system content block;
    /// other providers concatenate it onto the system string.
    #[allow(clippy::too_many_arguments)]
    async fn stream(
        &self,
        system: &str,
        system_date_suffix: &str,
        messages: &[Value],
        tools: &[ToolDef],
        thinking: &ThinkingConfig,
        response_schema: Option<&ResponseSchema>,
        max_tokens_override: Option<u32>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Chunk, LlmError>> + Send>>, LlmError>;

    /// Serialise `blocks` as a provider-native assistant message JSON value.
    ///
    /// Called by `run_with_tools` to append the assistant turn (including
    /// thinking blocks) to the conversation before the next user turn.
    fn assistant_message(&self, blocks: &[ContentBlock]) -> Value;

    /// Produce the provider-native user message(s) that carry tool results.
    ///
    /// Returns one or more message values to push onto the conversation.
    /// Anthropic batches all results into a single user message; OpenAI
    /// emits one `"tool"` role message per result.
    ///
    /// Each entry in `results` is `(tool_use_id, content, is_error)`.
    fn tool_result_messages(&self, results: &[(String, String, bool)]) -> Vec<Value>;

    /// The model identifier used by this provider (e.g. `"claude-sonnet-4-6"`).
    fn model_name(&self) -> &str;
}
