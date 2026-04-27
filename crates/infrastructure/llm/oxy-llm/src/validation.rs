//! Unified entry point for LLM API-key validation.
//!
//! Provider-specific probes (HTTP request, status interpretation) live in
//! the leaf provider crates. This module owns the dispatch — given a string
//! provider identifier from a request body, route to the right probe and
//! return a structured [`KeyValidationError`] suitable for surfacing to the
//! user.

use oxy_shared::KeyValidationError;

/// Providers we currently know how to probe for key validity.
///
/// Gemini and Ollama don't have entries because we haven't implemented
/// validation probes for them yet — `validate_provider_key` returns
/// `Unsupported` for any unknown provider so the caller can fall back to
/// "save without verifying" gracefully.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Anthropic,
    OpenAI,
}

impl ProviderKind {
    /// Parse a provider identifier as it appears in API request bodies and
    /// `vendor` fields (case-insensitive, ASCII).
    pub fn from_str_ci(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "anthropic" => Some(Self::Anthropic),
            "openai" => Some(Self::OpenAI),
            _ => None,
        }
    }
}

/// Validate an API key against the named provider. Returns `Ok(())` when
/// the provider accepts the key, or a structured error describing why the
/// probe failed.
///
/// Unknown providers (e.g. Gemini, Ollama — anything we haven't wired a
/// probe for yet) come back as `KeyValidationErrorKind::Unsupported`. That
/// variant is distinct from `Unreachable` so the user-facing message
/// doesn't falsely blame the network for a known feature gap; callers can
/// match on it to skip validation gracefully (e.g. the GitHub onboarding
/// flow saves vendors it can't probe without verification).
pub async fn validate_provider_key(provider: &str, api_key: &str) -> Result<(), KeyValidationError> {
    match ProviderKind::from_str_ci(provider) {
        Some(ProviderKind::Anthropic) => oxy_anthropic::validate_api_key(api_key).await,
        Some(ProviderKind::OpenAI) => oxy_openai::validate_api_key(api_key).await,
        None => Err(KeyValidationError::unsupported(provider)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_parses_known_names_case_insensitively() {
        assert_eq!(ProviderKind::from_str_ci("anthropic"), Some(ProviderKind::Anthropic));
        assert_eq!(ProviderKind::from_str_ci("Anthropic"), Some(ProviderKind::Anthropic));
        assert_eq!(ProviderKind::from_str_ci("  OPENAI  "), Some(ProviderKind::OpenAI));
    }

    #[test]
    fn provider_kind_returns_none_for_unknown() {
        assert_eq!(ProviderKind::from_str_ci("gemini"), None);
        assert_eq!(ProviderKind::from_str_ci(""), None);
    }

    #[tokio::test]
    async fn validate_provider_key_marks_unknown_provider_unsupported() {
        use oxy_shared::KeyValidationErrorKind;
        let err = validate_provider_key("gemini", "irrelevant").await.unwrap_err();
        assert_eq!(err.kind, KeyValidationErrorKind::Unsupported);
        // The user message must not blame reachability — Gemini works fine,
        // we just don't have a probe for it yet.
        assert!(!err.user_message().contains("could not be reached"));
        assert!(err.user_message().contains("gemini"));
    }
}
