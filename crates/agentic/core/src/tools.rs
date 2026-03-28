//! Core tool interface: [`ToolDef`] and [`ToolError`].
//!
//! [`ToolDef`] describes a single tool exposed to the LLM.
//! [`ToolError`] is returned when tool execution fails.
//!
//! Tools follow Unix philosophy: each tool does ONE thing, takes 1–2 parameters,
//! and returns structured data. The LLM composes them.

use serde_json::Value;

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
