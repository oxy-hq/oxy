//! [`ContextBudget`] — token-budget utility for prompt construction.
//!
//! LLM context windows are finite.  When assembling prompts from multiple
//! sources (schema, metric definitions, examples, retry context) the total
//! may exceed the model's input limit.  `ContextBudget` provides a simple
//! accounting layer:
//!
//! 1. Estimate how many tokens a string consumes.
//! 2. Reserve portions of the budget for each prompt section.
//! 3. Trim sections that would overflow their allocation.
//!
//! Token estimates are intentionally approximate (4 chars ≈ 1 token) —
//! exact counts require a full tokenizer and are not worth the overhead here.
//!
//! # Example
//!
//! ```rust
//! use agentic_analytics::context_budget::ContextBudget;
//!
//! let mut budget = ContextBudget::new(8_000);
//!
//! // Reserve 500 tokens for retry context (always shown in full).
//! let retry_text = "Prior error: column does not exist.";
//! let (fits, _) = budget.reserve("retry", retry_text);
//! assert!(fits);
//!
//! // Trim a long schema string to whatever remains.
//! let schema = "...very long schema...";
//! let trimmed = budget.trim_to_remaining(schema);
//! assert!(trimmed.len() <= schema.len());
//! ```

/// Rough token estimate: 4 UTF-8 bytes ≈ 1 token.
///
/// This is the same heuristic used by OpenAI's tiktoken docs for a quick
/// upper bound without running a real tokenizer.
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() + 3) / 4
}

/// Trim `text` so that its estimated token count does not exceed `max_tokens`.
///
/// Truncation happens on a character boundary.  A `"…"` suffix is appended
/// when the text is trimmed to signal that content was dropped.
pub fn trim_to_tokens(text: &str, max_tokens: usize) -> &str {
    if max_tokens == 0 {
        return "";
    }
    let max_chars = max_tokens * 4;
    if text.len() <= max_chars {
        return text;
    }
    // Walk back to a valid char boundary, leaving room for the ellipsis (3 bytes).
    let cut = max_chars.saturating_sub(3);
    let mut end = cut.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Token-budget accounting for prompt assembly.
///
/// Call [`reserve`] for each section of the prompt in priority order
/// (highest priority first).  Sections that overflow the remaining budget
/// are returned with `fits = false`; callers may then call
/// [`trim_to_remaining`] to get a trimmed version instead.
///
/// [`reserve`]: ContextBudget::reserve
/// [`trim_to_remaining`]: ContextBudget::trim_to_remaining
#[derive(Debug, Clone)]
pub struct ContextBudget {
    total: usize,
    used: usize,
}

impl ContextBudget {
    /// Create a new budget with a `total` token limit.
    pub fn new(total: usize) -> Self {
        Self { total, used: 0 }
    }

    /// Tokens remaining in the budget.
    pub fn remaining(&self) -> usize {
        self.total.saturating_sub(self.used)
    }

    /// Tokens consumed so far.
    pub fn used(&self) -> usize {
        self.used
    }

    /// Total token limit.
    pub fn total(&self) -> usize {
        self.total
    }

    /// Reserve tokens for `text`.
    ///
    /// Returns `(fits, tokens_consumed)`.  When `fits` is `true` the full
    /// text was accounted for.  When `fits` is `false` only the remaining
    /// budget was consumed (so the text would need to be trimmed).
    pub fn reserve(&mut self, _label: &str, text: &str) -> (bool, usize) {
        let needed = estimate_tokens(text);
        let remaining = self.remaining();
        if needed <= remaining {
            self.used += needed;
            (true, needed)
        } else {
            self.used += remaining;
            (false, remaining)
        }
    }

    /// Trim `text` to fit within the remaining token budget, then consume it.
    ///
    /// If the full text fits, it is returned unchanged.  Otherwise a
    /// [`trim_to_tokens`] slice is returned.
    pub fn trim_to_remaining<'a>(&mut self, text: &'a str) -> &'a str {
        let remaining = self.remaining();
        let trimmed = trim_to_tokens(text, remaining);
        self.used += estimate_tokens(trimmed);
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn estimate_tokens_four_chars_is_one_token() {
        assert_eq!(estimate_tokens("abcd"), 1);
    }

    #[test]
    fn estimate_tokens_rounds_up() {
        // 5 chars → ceil(5/4) = 2
        assert_eq!(estimate_tokens("abcde"), 2);
    }

    #[test]
    fn trim_to_tokens_fits() {
        let text = "hello";
        assert_eq!(trim_to_tokens(text, 100), text);
    }

    #[test]
    fn trim_to_tokens_truncates() {
        let text = "abcdefghij"; // 10 chars, 3 tokens
        let result = trim_to_tokens(text, 1); // 4 chars max, minus 3 for ellipsis = 1 char
        assert!(result.len() < text.len());
    }

    #[test]
    fn trim_to_tokens_zero_budget_returns_empty() {
        assert_eq!(trim_to_tokens("hello", 0), "");
    }

    #[test]
    fn budget_reserve_within_limit() {
        let mut b = ContextBudget::new(100);
        let (fits, _) = b.reserve("a", "hello world"); // ~3 tokens
        assert!(fits);
        assert!(b.remaining() < 100);
    }

    #[test]
    fn budget_reserve_overflow() {
        let mut b = ContextBudget::new(1); // only 1 token
                                           // "hello world" is ~3 tokens → won't fit
        let (fits, consumed) = b.reserve("a", "hello world");
        assert!(!fits);
        assert_eq!(consumed, 1); // consumed what remained
        assert_eq!(b.remaining(), 0);
    }

    #[test]
    fn budget_trim_to_remaining() {
        let mut b = ContextBudget::new(5); // 5 tokens = 20 chars
        let long = "a".repeat(100);
        let trimmed = b.trim_to_remaining(&long);
        assert!(trimmed.len() < long.len());
        assert_eq!(b.remaining(), 0);
    }

    #[test]
    fn budget_remaining_never_underflows() {
        let mut b = ContextBudget::new(2);
        b.reserve("x", &"a".repeat(1000));
        assert_eq!(b.remaining(), 0);
    }
}
