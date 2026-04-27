//! Shared error type for LLM provider API-key validation probes.
//!
//! Each provider crate (`oxy-anthropic`, `oxy-openai`, …) returns this error
//! from its `validate_api_key` helper so the higher-level dispatcher in
//! `oxy-llm` and the onboarding HTTP handler can produce a uniform
//! user-facing message without re-encoding provider conventions.

/// Reason a key validation probe failed. Each bucket maps to a distinct user
/// message — keeping them structured (rather than collapsing to a single
/// `String`) lets callers act differently per case in the future (e.g.
/// retry-with-backoff on `RateLimited`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyValidationErrorKind {
    /// The provider returned 401 / 403 — the key was rejected.
    Rejected,
    /// The provider returned 429 — request was throttled, the key may still
    /// be valid.
    RateLimited,
    /// Anything else (network failure, 5xx, unexpected status). Detail is a
    /// short technical summary suitable for logs and as a parenthetical in
    /// the user-facing message.
    Unreachable(String),
    /// We don't have a validation probe implemented for this provider yet
    /// (e.g. Gemini, Ollama). Distinct from `Unreachable` so the user
    /// message doesn't falsely blame network reachability for what is
    /// really a feature gap.
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyValidationError {
    /// Human-readable provider label used when formatting the user message,
    /// e.g. `"Anthropic"` or `"OpenAI"`.
    pub provider_label: String,
    pub kind: KeyValidationErrorKind,
}

impl KeyValidationError {
    pub fn rejected(provider_label: impl Into<String>) -> Self {
        Self {
            provider_label: provider_label.into(),
            kind: KeyValidationErrorKind::Rejected,
        }
    }

    pub fn rate_limited(provider_label: impl Into<String>) -> Self {
        Self {
            provider_label: provider_label.into(),
            kind: KeyValidationErrorKind::RateLimited,
        }
    }

    pub fn unreachable(provider_label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            provider_label: provider_label.into(),
            kind: KeyValidationErrorKind::Unreachable(detail.into()),
        }
    }

    pub fn unsupported(provider_label: impl Into<String>) -> Self {
        Self {
            provider_label: provider_label.into(),
            kind: KeyValidationErrorKind::Unsupported,
        }
    }

    /// Render the error as the inline message we surface in onboarding. Kept
    /// next to the data so every caller produces identical wording.
    pub fn user_message(&self) -> String {
        match &self.kind {
            KeyValidationErrorKind::Rejected => format!(
                "{} rejected the API key. Double-check the value you pasted and try again.",
                self.provider_label
            ),
            KeyValidationErrorKind::RateLimited => format!(
                "{} is rate-limiting key validation right now. Wait a moment and try again — the key may still be valid.",
                self.provider_label
            ),
            KeyValidationErrorKind::Unreachable(detail) => format!(
                "{} could not be reached ({}). The key may be invalid or the service is unreachable.",
                self.provider_label, detail
            ),
            KeyValidationErrorKind::Unsupported => format!(
                "Validation isn't supported for {} keys yet. The key was not verified before saving.",
                self.provider_label
            ),
        }
    }
}

impl std::fmt::Display for KeyValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.user_message())
    }
}

impl std::error::Error for KeyValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_message_blames_the_key() {
        let err = KeyValidationError::rejected("Anthropic");
        let msg = err.user_message();
        assert!(msg.contains("Anthropic rejected the API key"));
    }

    #[test]
    fn rate_limited_message_does_not_blame_the_key() {
        let err = KeyValidationError::rate_limited("OpenAI");
        let msg = err.user_message();
        assert!(msg.contains("rate-limiting"));
        // Critical: a throttled request must NOT push the user to re-paste a
        // valid key. If this assertion regresses, the message is misleading.
        assert!(!msg.contains("rejected"));
    }

    #[test]
    fn unreachable_message_includes_detail() {
        let err = KeyValidationError::unreachable("OpenAI", "HTTP 503");
        assert!(err.user_message().contains("HTTP 503"));
    }

    #[test]
    fn unsupported_message_does_not_blame_reachability() {
        let err = KeyValidationError::unsupported("Gemini");
        let msg = err.user_message();
        // The original Unreachable message claimed the provider "could not be
        // reached", which was misleading for a known feature gap. The
        // dedicated Unsupported variant must avoid both that wording and
        // anything that suggests the user should re-paste the key.
        assert!(!msg.contains("could not be reached"));
        assert!(!msg.contains("rejected"));
        assert!(msg.contains("Gemini"));
    }
}
