//! OpenAI API-key validation probe.
//!
//! Hits OpenAI's `/v1/models` listing endpoint (zero token cost) to decide
//! whether a key would be accepted. Uses `Authorization: Bearer`, the same
//! auth scheme the production OpenAI client (`async-openai`) sends with
//! every request, so a key that passes this probe authenticates the same
//! way against chat completions.

use std::sync::OnceLock;
use std::time::Duration;

use oxy_shared::KeyValidationError;

use crate::{OPENAI_API_URL, VENDOR_LABEL};

/// Shared `reqwest::Client` for key probes. See the Anthropic probe for the
/// reasoning behind reusing one client.
fn probe_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build shared OpenAI key-probe HTTP client")
    })
}

/// Verify an OpenAI API key by listing models. Returns `Ok(())` when the
/// provider accepts the key, otherwise a structured [`KeyValidationError`]
/// suitable for surfacing inline to the user.
pub async fn validate_api_key(api_key: &str) -> Result<(), KeyValidationError> {
    validate_api_key_at(api_key, OPENAI_API_URL).await
}

/// Internal version of [`validate_api_key`] with a configurable base URL,
/// so wiremock-backed tests can exercise the full request path (URL +
/// auth + status interpretation) instead of re-implementing the request.
async fn validate_api_key_at(api_key: &str, base_url: &str) -> Result<(), KeyValidationError> {
    let url = format!("{base_url}/models");
    let response = probe_client()
        .get(url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|e| KeyValidationError::unreachable(VENDOR_LABEL, e.to_string()))?;

    interpret_status(response.status())
}

/// Map a probe response status to the validation outcome. Pure helper so the
/// branch coverage can be unit-tested without spinning up a fake server.
pub(crate) fn interpret_status(status: reqwest::StatusCode) -> Result<(), KeyValidationError> {
    use reqwest::StatusCode;
    if status.is_success() {
        return Ok(());
    }
    Err(match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            KeyValidationError::rejected(VENDOR_LABEL)
        }
        StatusCode::TOO_MANY_REQUESTS => KeyValidationError::rate_limited(VENDOR_LABEL),
        other => KeyValidationError::unreachable(VENDOR_LABEL, format!("HTTP {other}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxy_shared::KeyValidationErrorKind;
    use reqwest::StatusCode;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn interpret_status_treats_2xx_as_valid() {
        assert!(interpret_status(StatusCode::OK).is_ok());
        assert!(interpret_status(StatusCode::NO_CONTENT).is_ok());
    }

    #[test]
    fn interpret_status_flags_auth_failures_as_rejected() {
        let err = interpret_status(StatusCode::UNAUTHORIZED).unwrap_err();
        assert_eq!(err.kind, KeyValidationErrorKind::Rejected);
        let err = interpret_status(StatusCode::FORBIDDEN).unwrap_err();
        assert_eq!(err.kind, KeyValidationErrorKind::Rejected);
    }

    #[test]
    fn interpret_status_calls_out_rate_limits() {
        let err = interpret_status(StatusCode::TOO_MANY_REQUESTS).unwrap_err();
        assert_eq!(err.kind, KeyValidationErrorKind::RateLimited);
    }

    #[test]
    fn interpret_status_falls_back_for_other_errors() {
        let err = interpret_status(StatusCode::INTERNAL_SERVER_ERROR).unwrap_err();
        match err.kind {
            KeyValidationErrorKind::Unreachable(detail) => assert!(detail.contains("500")),
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }

    /// End-to-end smoke test of `validate_api_key` against a wiremock
    /// server. The mock asserts on the URL path and `Authorization: Bearer`
    /// header — the same auth scheme `async-openai` uses, so a regression
    /// here would also break chat-completion auth.
    #[tokio::test]
    async fn validate_api_key_sends_expected_request_on_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .and(header("authorization", "Bearer sk-test"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let result = validate_api_key_at("sk-test", &server.uri()).await;
        assert!(result.is_ok(), "expected success, got {result:?}");
    }

    /// Mirrors the success test for the rejection path so the
    /// status-handling branch is exercised through the real probe code.
    #[tokio::test]
    async fn validate_api_key_surfaces_rejection_on_401() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;

        let err = validate_api_key_at("sk-test", &server.uri())
            .await
            .expect_err("expected rejection");
        assert_eq!(err.kind, KeyValidationErrorKind::Rejected);
    }
}
