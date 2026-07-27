/// Errors returned by [`LlmClient`] and [`LlmProvider`] calls.
#[derive(Debug)]
pub enum LlmError {
    /// HTTP transport or server error.
    Http(String),
    /// Transient transport failure (connection error, timeout, or HTTP 5xx).
    /// Distinct from [`LlmError::Http`] because retrying after a backoff delay
    /// may succeed; solvers retry this on the same bounded-backoff path as
    /// [`LlmError::RateLimit`].
    Transient(String),
    /// Authentication failure (bad or missing API key).
    Auth(String),
    /// Rate limit exceeded (HTTP 429). Retrying after a backoff delay may
    /// succeed. `retry_after` carries the provider's `Retry-After` hint
    /// (delta-seconds form) when present, so the solver can honor the
    /// provider's requested delay instead of a blind exponential backoff.
    RateLimit {
        message: String,
        retry_after: Option<std::time::Duration>,
    },
    /// Response could not be parsed.
    Parse(String),
    /// The model produced thinking/reasoning but no text output — likely
    /// hit `max_tokens` during the thinking phase.
    EmptyResponse { reason: String },
    /// The `ask_user` tool was called with a [`DeferredInputProvider`] — the
    /// run must suspend and resume on the next user turn.
    ///
    /// `prior_messages` contains the full provider-native message history
    /// accumulated up to and including the assistant turn with the `ask_user`
    /// tool call.  The caller must persist this and pass it back to
    /// [`LlmClient::build_resume_messages`] on resume so the LLM retains
    /// context of any tool rounds that happened before the suspension.
    ///
    /// [`DeferredInputProvider`]: agentic_core::human_input::DeferredInputProvider
    Suspended {
        prompt: String,
        suggestions: Vec<String>,
        /// Full message history up to (and including) the `ask_user` assistant
        /// turn.  Provider-native JSON; opaque outside `agentic-analytics`.
        prior_messages: Vec<serde_json::Value>,
    },
    /// The model hit the token limit while generating text output.
    ///
    /// `partial_text` is the truncated response produced so far.
    /// `current_max_tokens` is the budget that was exhausted.
    /// `prior_messages` is the full history **including** the truncated
    /// assistant turn appended at the end — pass it to
    /// [`LlmClient::build_continue_messages`] on resume with a doubled
    /// `max_tokens_override`.
    MaxTokensReached {
        partial_text: String,
        current_max_tokens: u32,
        prior_messages: Vec<serde_json::Value>,
    },
    /// The tool loop consumed all configured rounds before producing a final
    /// answer.
    ///
    /// `prior_messages` is the message history at the point the limit was hit
    /// (before the model's unanswered request for more tools).  Pass it to
    /// [`LlmClient::build_continue_messages`] on resume with an increased
    /// `max_tool_rounds`.
    MaxToolRoundsReached {
        rounds: u32,
        prior_messages: Vec<serde_json::Value>,
    },
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::Http(msg) => write!(f, "HTTP error: {msg}"),
            LlmError::Transient(msg) => write!(f, "transient error: {msg}"),
            LlmError::Auth(msg) => write!(f, "auth error: {msg}"),
            LlmError::RateLimit { message, .. } => {
                write!(f, "rate limit exceeded: {message}")
            }
            LlmError::Parse(msg) => write!(f, "parse error: {msg}"),
            LlmError::EmptyResponse { reason } => {
                write!(f, "empty response from model: {reason}")
            }
            LlmError::Suspended { prompt, .. } => {
                write!(f, "ask_user suspended: {prompt}")
            }
            LlmError::MaxTokensReached {
                current_max_tokens, ..
            } => {
                write!(
                    f,
                    "model hit token limit ({current_max_tokens} tokens); response truncated"
                )
            }
            LlmError::MaxToolRoundsReached { rounds, .. } => {
                write!(
                    f,
                    "tool loop exhausted {rounds} rounds without final answer"
                )
            }
        }
    }
}

impl std::error::Error for LlmError {}
