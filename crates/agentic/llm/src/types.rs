use serde_json::Value;

// ── InitialMessages ───────────────────────────────────────────────────────────

/// The opening message(s) for a [`LlmClient::run_with_tools`] call.
///
/// - [`User`] — the normal path: the user's raw question, sent as a single
///   `{"role":"user","content":…}` message.
/// - [`Messages`] — pre-built message list, used on resume to inject the
///   synthetic `[user, assistant(ask_user), tool_result(answer)]` exchange
///   so the LLM sees the prior question/answer before continuing.
///
/// [`User`]: InitialMessages::User
/// [`Messages`]: InitialMessages::Messages
pub enum InitialMessages {
    /// A single user message (plain question text).
    User(String),
    /// A pre-built list of provider-native JSON message objects.
    ///
    /// Used for synthetic re-entry on resume: the caller constructs the
    /// `[user_msg, assistant(ask_user), tool_result(answer)]` sequence using
    /// [`LlmProvider::assistant_message`] and [`LlmProvider::tool_result_messages`],
    /// then passes it here.  The tool loop starts at round 0 with these messages
    /// already in the history.
    Messages(Vec<serde_json::Value>),
}

impl From<&str> for InitialMessages {
    fn from(s: &str) -> Self {
        InitialMessages::User(s.to_string())
    }
}

impl From<String> for InitialMessages {
    fn from(s: String) -> Self {
        InitialMessages::User(s)
    }
}

impl From<&String> for InitialMessages {
    fn from(s: &String) -> Self {
        InitialMessages::User(s.clone())
    }
}

// ── Thinking config ───────────────────────────────────────────────────────────

/// Reasoning effort level for OpenAI o-series models.
#[derive(Debug, Clone)]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
}

impl ReasoningEffort {
    pub(super) fn as_str(&self) -> &'static str {
        match self {
            ReasoningEffort::Low => "low",
            ReasoningEffort::Medium => "medium",
            ReasoningEffort::High => "high",
        }
    }
}

/// Thinking / reasoning configuration passed to each LLM call.
///
/// The variant chosen here controls what the provider sends in the request.
/// Pick the right variant for your model family:
///
/// | Variant | Models |
/// |---------|--------|
/// | `Disabled` | Any model, no extended thinking |
/// | `Adaptive` | Claude 4.6+ (model decides when/how much to think) |
/// | `Manual` | Claude 3.x / earlier Claude 4 (explicit token budget) |
/// | `Effort` | OpenAI o-series (low / medium / high effort) |
#[derive(Debug, Clone, Default)]
pub enum ThinkingConfig {
    /// No extended thinking (default).
    #[default]
    Disabled,
    /// Claude 4.6+ adaptive thinking: the model decides when to think and for
    /// how long.  Sends `"thinking": {"type": "adaptive"}` in the request.
    Adaptive,
    /// Explicit thinking budget for Claude 3.x / earlier Claude 4 models.
    /// Sends `"thinking": {"type": "enabled", "budget_tokens": N}`.
    Manual { budget_tokens: u32 },
    /// OpenAI o-series reasoning effort.
    /// Sends `"reasoning_effort": "low"|"medium"|"high"`.
    Effort(ReasoningEffort),
}

// ── Structured output ─────────────────────────────────────────────────────────

/// Describes a structured output schema for constrained LLM responses.
///
/// When set on [`ToolLoopConfig`], the provider uses its native constrained
/// decoding mechanism to guarantee the response matches this schema:
/// - **Anthropic**: native `output_config.format` (structured outputs API).
/// - **OpenAI**: `response_format: {type: "json_schema", ...}` with `strict: true`.
#[derive(Debug, Clone)]
pub struct ResponseSchema {
    /// Machine-readable name for the response schema (e.g. `"clarify_response"`).
    pub name: String,
    /// JSON Schema object describing the expected response shape.
    pub schema: Value,
}

// ── Content blocks ────────────────────────────────────────────────────────────

/// A parsed content block from an LLM response.
///
/// Thinking and redacted-thinking blocks store the **complete** provider-native
/// JSON object in `provider_data` so they can be passed back verbatim during
/// tool-use loops.  Callers must not inspect or mutate `provider_data`.
///
/// # Lifetime constraint
///
/// Thinking blocks MUST NOT cross FSM state boundaries.  The orchestrator
/// discards [`LlmOutput::raw_content_blocks`] when transitioning between
/// states.  Only the human-readable thinking summary (carried by
/// [`CoreEvent::ThinkingToken`] events) may be retained for logging / display.
#[derive(Debug, Clone)]
pub enum ContentBlock {
    /// Extended thinking block.  Contains the provider's encrypted blob
    /// (Anthropic: `signature`; OpenAI: `encrypted_content`).
    /// Pass back verbatim in the next assistant turn.
    Thinking { provider_data: Value },
    /// Redacted thinking block (Claude-specific).  Opaque; pass back verbatim.
    RedactedThinking { provider_data: Value },
    /// Ordinary text output.
    Text { text: String },
    /// A tool invocation requested by the model.
    ToolUse {
        id: String,
        name: String,
        input: Value,
        /// Complete provider-native JSON for this tool call.  When present,
        /// [`LlmProvider::assistant_message`] should pass it back verbatim
        /// instead of reconstructing from the parsed fields.  This is
        /// required by the OpenAI Responses API, which expects the full
        /// output item (including `id`, `status`, etc.) in the next input.
        provider_data: Option<Value>,
    },
}

// ── Streaming chunk types ─────────────────────────────────────────────────────

/// A complete tool invocation carried in a [`Chunk::ToolCall`].
///
/// Emitted once per tool call after the full input JSON has been accumulated
/// from the stream.
#[derive(Debug, Clone)]
pub struct ToolCallChunk {
    pub id: String,
    pub name: String,
    pub input: Value,
    /// Complete provider-native JSON for this tool call, stored for verbatim
    /// passback during tool-use loops.  `None` for providers that reconstruct
    /// the wire format from the parsed fields (e.g. Anthropic).
    pub provider_data: Option<Value>,
}

/// A single item in the streaming output from an [`LlmProvider`].
#[derive(Debug)]
pub enum Chunk {
    /// Human-readable thinking text (Anthropic thinking block / OpenAI
    /// reasoning summary).  May arrive in multiple chunks.  Encrypted blobs
    /// are emitted separately as [`Chunk::RawBlock`].
    ThinkingSummary(String),
    /// A content text token.
    Text(String),
    /// A complete tool call, emitted after the full input JSON is received.
    ToolCall(ToolCallChunk),
    /// An opaque provider blob (encrypted thinking / signature) that must be
    /// passed back verbatim in tool continuation turns.
    RawBlock(ContentBlock),
    /// The stream finished; carries total token usage.
    Done(Usage),
}

// ── Output types ──────────────────────────────────────────────────────────────

/// Why the model stopped generating.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum StopReason {
    /// Normal completion — the model emitted a stop token.
    #[default]
    EndTurn,
    /// The response was truncated because `max_tokens` was reached.
    MaxTokens,
    /// The model elected to use a tool.
    ToolUse,
}

/// Token usage reported by the provider for a single completion.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    /// Why the model stopped; populated by provider stream parsers.
    pub stop_reason: StopReason,
}

/// The final output produced by a completion or tool-use loop.
#[derive(Debug, Clone)]
pub struct LlmOutput {
    /// The assistant's final text response.
    pub text: String,
    /// Human-readable summary of thinking blocks seen in the final response
    /// round, for display / event logging only.  Never re-injected into the LLM.
    pub thinking_summary: Option<String>,
    /// Raw content blocks from the **final** response round.  The orchestrator
    /// MUST discard these when transitioning between FSM states.
    pub raw_content_blocks: Vec<ContentBlock>,
    /// Structured JSON response extracted from a forced tool call (Anthropic)
    /// or constrained decoding (OpenAI). `None` when no `response_schema` was set.
    pub structured_response: Option<Value>,
    /// Every tool call made during the loop, in order: `(tool_name, input_params)`.
    ///
    /// Lets callers inspect tool-use side-effects after the loop without
    /// resorting to `Arc<Mutex>` captures inside the tool executor closure.
    pub tool_calls: Vec<(String, Value)>,
}

// ── Tool loop configuration ───────────────────────────────────────────────────

/// Configuration for [`LlmClient::run_with_tools`].
#[derive(Debug, Clone)]
pub struct ToolLoopConfig {
    /// Maximum number of tool-call rounds before returning
    /// [`LlmError::MaxToolRoundsExceeded`].
    pub max_tool_rounds: u32,
    /// Pipeline state name, used for [`CoreEvent::LlmStart`] /
    /// [`CoreEvent::LlmEnd`] / thinking event fields.
    pub state: String,
    /// Thinking / reasoning mode for this state's LLM calls.
    pub thinking: ThinkingConfig,
    /// Optional JSON Schema for structured output.
    /// When set, the provider uses constrained decoding to guarantee
    /// the response matches this schema exactly.
    pub response_schema: Option<ResponseSchema>,
    /// Optional explicit `max_tokens` override.  When `Some`, the provider
    /// uses this value instead of computing one from `ThinkingConfig`.
    /// Used by the truncation-retry logic in [`LlmClient::run_with_tools`].
    pub max_tokens_override: Option<u32>,
    /// When inside a concurrent fan-out, identifies which sub-spec this
    /// tool loop belongs to.  Propagated to all emitted [`CoreEvent`]s.
    pub sub_spec_index: Option<usize>,
}

impl Default for ToolLoopConfig {
    fn default() -> Self {
        Self {
            max_tool_rounds: 5,
            state: String::new(),
            thinking: ThinkingConfig::Disabled,
            response_schema: None,
            max_tokens_override: None,
            sub_spec_index: None,
        }
    }
}
