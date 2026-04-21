//! Consistency evaluation trait for picking the most agreed-upon answer
//! from N candidates.
//!
//! Define the trait here (core), implement it in infrastructure crates
//! (e.g. `agentic-llm`), and inject via pipeline.

use async_trait::async_trait;

/// Result of a consistency evaluation.
#[derive(Debug, Clone)]
pub struct EvalResult {
    /// 0-based index of the selected answer in the input slice.
    pub selected_index: usize,
    /// Confidence score in 0.0–1.0 (e.g. pairwise wins / max possible wins).
    pub score: f64,
    /// Human-readable explanation (may be empty for pairwise).
    pub reasoning: String,
}

/// Error from a consistency evaluation.
#[derive(Debug)]
pub struct EvalError(pub String);

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for EvalError {}

/// Picks the most agreed-upon answer from N candidates.
///
/// Implementations may use LLM pairwise comparison, embedding similarity,
/// or any other strategy. The trait is intentionally minimal so it can be
/// used in workflow orchestrators, eval harnesses, and agent tests alike.
#[async_trait]
pub trait ConsistencyEvaluator: Send + Sync {
    /// Evaluate `answers` against `question` and return the winner.
    ///
    /// `custom_prompt` overrides the implementation's default evaluation
    /// prompt when provided.
    async fn evaluate(
        &self,
        question: &str,
        answers: &[String],
        custom_prompt: Option<&str>,
    ) -> Result<EvalResult, EvalError>;
}
