pub(super) const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
pub(super) const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Beta feature flag required for extended thinking / streaming thinking.
/// Without this header, the API silently ignores the `thinking` body
/// parameter and no thinking blocks appear in the SSE stream.
pub(super) const ANTHROPIC_THINKING_BETA: &str = "interleaved-thinking-2025-05-14";
pub(super) const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

/// The default model used when constructing an [`LlmClient`] via [`LlmClient::new`].
pub const DEFAULT_MODEL: &str = "claude-opus-4-6";

pub(super) const DEFAULT_MAX_TOKENS: u32 = 4096;
/// Higher token cap used when extended thinking is enabled.  Thinking
/// output (especially Manual budgets) can consume thousands of tokens;
/// the text response needs room on top of that.
pub(super) const THINKING_MAX_TOKENS: u32 = 16384;
