//! [`QuestionType`], [`AnalyticsIntent`], and related human-facing domain types.

use agentic_core::HumanInputQuestion;
use serde::{Deserialize, Serialize};

use super::query_request::{QueryRequestItem, SpecHint};

// ---------------------------------------------------------------------------
// Intent
// ---------------------------------------------------------------------------

/// The type of analytical question being asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuestionType {
    /// "How has X changed over time?"
    Trend,
    /// "How does X compare to Y?"
    Comparison,
    /// "What is X broken down by Y?"
    Breakdown,
    /// "What is the current value of X?"
    SingleValue,
    /// "How is X distributed?"
    Distribution,
    /// A general question that does not require a SQL query — e.g. "what tables
    /// do you have?", "what metrics can you track?", or any conversational
    /// follow-up that the system can answer directly from schema context.
    GeneralInquiry,
}

/// A single completed question–answer exchange, kept for follow-up context.
///
/// Passed in as part of [`AnalyticsIntent::history`] on subsequent questions
/// so that the Clarify and Interpret stages can reference prior exchanges when
/// resolving ambiguities or phrasing answers.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConversationTurn {
    /// The natural-language question posed by the user.
    pub question: String,
    /// The natural-language answer produced by the Interpret stage.
    pub answer: String,
}
/// The kind of semantic member that is missing from the catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingMemberKind {
    Measure,
    Dimension,
}

/// A semantic member that the user's question requires but does not exist in
/// the catalog. Reported by the triage LLM when `search_catalog` cannot find
/// a matching measure or dimension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingMember {
    /// Suggested member name (e.g. `"revenue_per_customer"`).
    pub name: String,
    /// Whether this is a measure or dimension.
    pub kind: MissingMemberKind,
    /// Natural-language description of what the member should represent.
    pub description: String,
}

/// Lightweight hypothesis produced by the Triage sub-phase of Clarify.
///
/// Triage runs *without* tools or column-level schema — it only sees table
/// names.  Its job is to narrow the search space before the heavier Ground
/// sub-phase explores columns via tool calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainHypothesis {
    /// Natural-language summary of what the user is asking about.
    /// e.g. "The user wants to see their weight trend over recent weeks."
    pub summary: String,
    /// Broad question category chosen *before* seeing column details.
    pub question_type: QuestionType,
    /// Inferred time scope, if any (e.g. "last 30 days", "this year").
    #[serde(default)]
    pub time_scope: Option<String>,
    /// How confident the model is in its interpretation (0.0–1.0).
    #[serde(default = "default_confidence")]
    pub confidence: f32,
    /// Language-level ambiguities in the user's question that cannot be
    /// resolved without asking the user — e.g. "unclear which metric 'progress'
    /// refers to" or "time range is unspecified".  Empty when the question is
    /// unambiguous.
    #[serde(default)]
    pub ambiguities: Vec<String>,
    /// Structured version of `ambiguities` — each entry is a question with
    /// LLM-generated suggestions.  Populated when the triage schema includes
    /// `ambiguity_questions`; falls back to constructing from `ambiguities`
    /// with empty suggestions when absent.
    #[serde(default)]
    pub ambiguity_questions: Vec<HumanInputQuestion>,
    /// Path of a matching procedure selected by triage, if any.
    /// When set, the pipeline executes the procedure instead of generating SQL.
    #[serde(default)]
    pub selected_procedure_path: Option<String>,

    /// If the LLM found all required semantic members in the catalog, it
    /// constructs a `QueryRequestItem` here to attempt a fast airlayer compile
    /// in the Clarifying stage.  `None` when the LLM is not confident enough
    /// or the catalog doesn't have the right members.
    #[serde(default)]
    pub semantic_query: Option<QueryRequestItem>,

    /// How confident the LLM is that the `semantic_query` members are correct
    /// (0.0–1.0).  Only meaningful when `semantic_query` is `Some`.
    #[serde(default)]
    pub semantic_confidence: f32,

    /// Semantic members that the user's question requires but that
    /// `search_catalog` could not find.  Populated by the triage LLM when
    /// coverage is partial.  The pipeline uses this to delegate creation of
    /// the missing members to the builder agent.
    #[serde(default)]
    pub missing_members: Vec<MissingMember>,
}

fn default_confidence() -> f32 {
    1.0
}

/// The user-facing analytics request produced by the Clarify stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsIntent {
    /// Original natural-language question.
    pub raw_question: String,
    /// One-sentence summary of the user's intent produced by triage.
    ///
    /// Provides a clearer, disambiguated description of what the user is
    /// asking — unlike `raw_question` which is the verbatim input.  Used by
    /// downstream stages (Specifying, Interpreting) so the LLM has a
    /// pre-digested understanding of the goal.
    #[serde(default)]
    pub summary: String,
    /// Classified question type.
    pub question_type: QuestionType,
    /// Metric names the user cares about (e.g. `["revenue", "orders"]`).
    pub metrics: Vec<String>,
    /// Grouping dimensions (e.g. `["region", "product_category"]`).
    pub dimensions: Vec<String>,
    /// Filter expressions in a simple DSL (e.g. `["date >= '2024-01-01'"]`).
    pub filters: Vec<String>,
    /// Prior question–answer turns from the current conversation session.
    ///
    /// Empty for the first question.  Populated by the caller before passing
    /// a follow-up question into [`Orchestrator::run`].  The Clarify and
    /// Interpret prompt builders inject this history so the LLM can resolve
    /// references like "compare that to last year" or maintain a consistent
    /// tone.
    #[serde(default)]
    pub history: Vec<ConversationTurn>,
    /// Prior query structure in airlayer grammar, if any.
    ///
    /// Set on back-edge retries (when a `QuerySpec` failed downstream) and on
    /// cross-turn follow-ups (the most recent completed run's query).  Injected
    /// into the Specify prompt so the LLM reuses the prior structure.
    #[serde(default)]
    pub spec_hint: Option<SpecHint>,
    /// Procedure file selected by the LLM during the Ground sub-phase.
    ///
    /// When the LLM calls `search_procedures` and finds a file that directly
    /// answers the question, it sets this path in its structured response.
    /// The Specifying stage short-circuits: it skips LLM resolution and
    /// emits a `QuerySpec` with `SolutionSource::Procedure { file_path }`
    /// so execution jumps straight to the Executing stage.
    #[serde(default)]
    pub selected_procedure: Option<std::path::PathBuf>,
    /// Best-effort semantic query produced by triage.
    ///
    /// Always populated — even when triage is not fully confident about the
    /// member paths.  The decision to take the semantic shortcut vs. fall
    /// through to Specifying is governed by `semantic_confidence`, not by
    /// whether this field is empty.  Carrying the query forward lets the
    /// Specifying stage reuse triage's catalog discoveries instead of
    /// re-searching from scratch.
    #[serde(default)]
    pub semantic_query: QueryRequestItem,
    /// How confident triage is that `semantic_query` members are correct
    /// (0.0–1.0).
    ///
    /// Values ≥ 0.85 trigger the semantic shortcut (compile locally, skip
    /// Specifying/Solving).  Lower values cause fall-through to Specifying,
    /// which still benefits from the `semantic_query` as a starting hint.
    #[serde(default)]
    pub semantic_confidence: f32,
}
