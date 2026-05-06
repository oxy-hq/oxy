//! Core tool interface: [`ToolDef`] and [`ToolError`].
//!
//! [`ToolDef`] describes a single tool exposed to the LLM.
//! [`ToolError`] is returned when tool execution fails.
//!
//! Tools follow Unix philosophy: each tool does ONE thing, takes 1–2 parameters,
//! and returns structured data. The LLM composes them.

use serde_json::Value;

// ── ToolOutput ────────────────────────────────────────────────────────────────

/// The result of a tool execution, carrying both a structured representation
/// (for the frontend / event stream) and an agent-friendly plain-text form
/// (sent back to the LLM as the tool result content).
///
/// Implement this trait on typed output structs to control exactly what the LLM
/// sees. [`Value`] implements it as a backward-compatible fallback (both methods
/// use the JSON serialization).
pub trait ToolOutput: Send + Sync {
    /// Plain-text description of the result returned to the LLM.
    fn to_agent_text(&self) -> String;
    /// Structured [`Value`] emitted in `CoreEvent::ToolResult` for the frontend.
    fn to_value(&self) -> Value;
}

impl ToolOutput for Value {
    fn to_agent_text(&self) -> String {
        self.to_string()
    }
    fn to_value(&self) -> Value {
        self.clone()
    }
}

// ── ToolDef ───────────────────────────────────────────────────────────────────

/// Describes a single tool that the LLM may invoke during a pipeline stage.
///
/// Tools are scoped per state — a state exposes only the tools relevant to
/// that stage of the pipeline.  This prevents the LLM from using, e.g.,
/// `explain_plan` during clarification.
///
/// # Example
///
/// ```rust
/// use agentic_core::tools::ToolDef;
/// use serde_json::json;
///
/// let t = ToolDef {
///     name: "search_catalog",
///     description: "Search the catalog for metrics and dimensions.",
///     parameters: json!({
///         "type": "object",
///         "properties": { "queries": { "type": "array", "items": { "type": "string" } } },
///         "required": ["queries"],
///     }),
/// };
/// assert_eq!(t.name, "search_catalog");
/// ```
#[derive(Debug, Clone)]
pub struct ToolDef {
    /// Machine-readable name (snake_case).  Sent to the LLM verbatim.
    pub name: &'static str,
    /// Human-readable description used in the LLM prompt.
    pub description: &'static str,
    /// JSON Schema object describing the tool's input parameters.
    ///
    /// Must be a `{"type":"object","properties":{...},"required":[...]}` shape
    /// compatible with the Anthropic `input_schema` format.
    pub parameters: Value,
    /// Whether to enable strict mode for this tool (provider-specific).
    /// Defaults to `true`. Set to `false` for tools whose schemas use union
    /// types (`["string", "null"]`) or other constructs incompatible with
    /// strict structured-output validation.
    pub strict: bool,
}

impl Default for ToolDef {
    fn default() -> Self {
        Self {
            name: "",
            description: "",
            parameters: Value::Null,
            strict: true,
        }
    }
}

// ── ToolError ─────────────────────────────────────────────────────────────────

/// Errors returned by tool execution.
#[derive(Debug)]
pub enum ToolError {
    /// The tool name is not registered for the current state.
    UnknownTool(String),
    /// A required parameter was absent or had the wrong type.
    BadParams(String),
    /// The tool ran but encountered a runtime error.
    Execution(String),
    /// The `ask_user` tool was invoked with a [`DeferredInputProvider`] — the
    /// pipeline must suspend and wait for the user's answer in a future turn.
    ///
    /// [`DeferredInputProvider`]: crate::human_input::DeferredInputProvider
    Suspended {
        prompt: String,
        suggestions: Vec<String>,
    },
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::UnknownTool(n) => write!(f, "unknown tool: {n}"),
            ToolError::BadParams(msg) => write!(f, "bad params: {msg}"),
            ToolError::Execution(msg) => write!(f, "execution error: {msg}"),
            ToolError::Suspended { prompt, .. } => write!(f, "ask_user suspended: {prompt}"),
        }
    }
}

impl std::error::Error for ToolError {}

// ── ask_user shared tool ─────────────────────────────────────────────────────

/// Shared `ask_user` tool definition used across domains (analytics, builder).
///
/// The LLM invokes this when it needs additional input from the user.
/// The tool executor checks the [`HumanInputProvider`]:
/// - CLI providers block and return the answer immediately.
/// - [`DeferredInputProvider`] returns `ToolError::Suspended`, suspending the pipeline.
///
/// [`HumanInputProvider`]: crate::human_input::HumanInputProvider
/// [`DeferredInputProvider`]: crate::human_input::DeferredInputProvider
pub fn ask_user_tool_def() -> ToolDef {
    ToolDef {
        name: "ask_user",
        description: "Ask the user a clarifying question when you need more information to proceed accurately. Always provide 2–4 concrete suggestions that cover the most likely answers.",
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "The question to ask the user."
                },
                "suggestions": {
                    "type": "array",
                    "description": "2–4 concrete suggested answers to guide the user. Always provide suggestions — they appear as clickable buttons in the UI.",
                    "items": { "type": "string" }
                }
            },
            "required": ["prompt", "suggestions"],
            "additionalProperties": false
        }),
        ..Default::default()
    }
}

/// Execute an `ask_user` tool call via the given [`HumanInputProvider`].
///
/// - `Ok(json!({"answer": ...}))` when the provider returns immediately (CLI).
/// - `Err(ToolError::Suspended { .. })` when the provider defers (server).
///
/// [`HumanInputProvider`]: crate::human_input::HumanInputProvider
pub fn handle_ask_user(
    params: &serde_json::Value,
    provider: &dyn crate::human_input::HumanInputProvider,
) -> Result<serde_json::Value, ToolError> {
    let prompt = params["prompt"].as_str().unwrap_or("").to_string();
    let suggestions: Vec<String> = params["suggestions"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    match provider.request_sync(&prompt, &suggestions) {
        Ok(answer) => Ok(serde_json::json!({ "answer": answer })),
        Err(()) => Err(ToolError::Suspended {
            prompt,
            suggestions,
        }),
    }
}
