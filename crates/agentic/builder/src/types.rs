//! Domain types for the builder FSM.

use agentic_core::domain::Domain;
use serde::{Deserialize, Serialize};

/// A tool call + result exchange within a prior conversation turn.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolExchange {
    pub name: String,
    /// JSON-encoded input parameters.
    pub input: String,
    /// Tool result output string.
    pub output: String,
}

/// A prior conversation turn (question + answer) for multi-turn context.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub question: String,
    pub answer: String,
    /// Tool calls made during this turn, in order.
    pub tool_exchanges: Vec<ToolExchange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderIntent {
    pub question: String,
    pub history: Vec<ConversationTurn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderSpec {
    pub question: String,
    pub history: Vec<ConversationTurn>,
}

impl From<BuilderIntent> for BuilderSpec {
    fn from(intent: BuilderIntent) -> Self {
        Self {
            question: intent.question,
            history: intent.history,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderSolution {
    pub question: String,
    pub history: Vec<ConversationTurn>,
    pub draft_text: String,
    pub tool_exchanges: Vec<ToolExchange>,
    /// Full provider-native message history from the solving phase.
    /// Passed to the interpreting call so it sees the complete, unabridged context.
    #[serde(default)]
    pub prior_messages: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderResult {
    pub question: String,
    pub history: Vec<ConversationTurn>,
    pub draft_text: String,
    pub tool_exchanges: Vec<ToolExchange>,
    /// Full provider-native message history from the solving phase.
    #[serde(default)]
    pub prior_messages: Vec<serde_json::Value>,
}

impl From<BuilderSolution> for BuilderResult {
    fn from(solution: BuilderSolution) -> Self {
        Self {
            question: solution.question,
            history: solution.history,
            draft_text: solution.draft_text,
            tool_exchanges: solution.tool_exchanges,
            prior_messages: solution.prior_messages,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderAnswer {
    pub text: String,
    pub tool_exchanges: Vec<ToolExchange>,
}

#[derive(Debug)]
pub enum BuilderError {
    Llm(String),
    NeedsUserInput { prompt: String },
    Resume(String),
}

impl std::fmt::Display for BuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuilderError::Llm(s) => write!(f, "LLM error: {s}"),
            BuilderError::NeedsUserInput { prompt } => write!(f, "needs user input: {prompt}"),
            BuilderError::Resume(s) => write!(f, "resume error: {s}"),
        }
    }
}

pub struct BuilderDomain;

impl Domain for BuilderDomain {
    type Intent = BuilderIntent;
    type Spec = BuilderSpec;
    type Solution = BuilderSolution;
    type Result = BuilderResult;
    type Answer = BuilderAnswer;
    type Catalog = ();
    type Error = BuilderError;
}
