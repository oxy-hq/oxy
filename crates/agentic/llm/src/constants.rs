use std::time::Duration;

use crate::LlmError;

pub(super) const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
pub(super) const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Beta feature flag required for extended thinking / streaming thinking.
/// Without this header, the API silently ignores the `thinking` body
/// parameter and no thinking blocks appear in the SSE stream.
pub(super) const ANTHROPIC_THINKING_BETA: &str = "interleaved-thinking-2025-05-14";
pub(super) const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

/// The default model used when constructing an [`LlmClient`] via [`LlmClient::new`].
pub const DEFAULT_MODEL: &str = "claude-opus-4-8";

pub(super) const DEFAULT_MAX_TOKENS: u32 = 4096;
/// Higher token cap used when extended thinking is enabled.  Thinking
/// output (especially Manual budgets) can consume thousands of tokens;
/// the text response needs room on top of that.
pub(super) const THINKING_MAX_TOKENS: u32 = 16384;

/// Connect timeout for LLM provider HTTP clients.  Bounds only TCP/TLS
/// connection establishment — **not** the (potentially long) streaming body.
pub(super) const LLM_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Idle read timeout for streaming LLM responses.  reqwest resets this on every
/// chunk read, so it bounds the gap *between* bytes rather than the total
/// stream duration: a legitimately long generation is fine as long as tokens
/// keep arriving, while a silently dead connection is cut instead of hanging
/// forever.  A hard overall `.timeout()` is deliberately avoided because it
/// would truncate long legitimate streams.
pub(super) const LLM_READ_TIMEOUT: Duration = Duration::from_secs(120);

/// Build a reqwest client for LLM providers with connect + idle-read timeouts.
///
/// Falls back to a default (timeout-less) client if the builder fails, which is
/// unreachable in practice for these settings — the fallback only exists so the
/// infallible `Provider::new` constructors keep their signatures.
pub(super) fn build_llm_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(LLM_CONNECT_TIMEOUT)
        .read_timeout(LLM_READ_TIMEOUT)
        .build()
        .unwrap_or_else(|err| {
            tracing::warn!(
                error = %err,
                "failed to build LLM HTTP client with timeouts; using default client"
            );
            reqwest::Client::new()
        })
}

/// Classify a reqwest transport error from `.send()` into an [`LlmError`].
///
/// Connection failures and timeouts are transient — worth a bounded retry —
/// and map to [`LlmError::Transient`].  Everything else surfaces as
/// [`LlmError::Http`] (non-retryable).
pub(super) fn send_error_to_llm(err: reqwest::Error) -> LlmError {
    if err.is_connect() || err.is_timeout() {
        LlmError::Transient(err.to_string())
    } else {
        LlmError::Http(err.to_string())
    }
}
