//! `users.lookupByEmail`: who an address belongs to in a Slack workspace.
//!
//! Its own module because its answer is three-way, which the crate's shared
//! `check_ok` cannot express: Slack answers an unknown address with
//! `users_not_found`, an ordinary "no", not a fault.

use oxy_shared::errors::OxyError;
use serde::Deserialize;
use serde_json::Value;

use crate::SlackClient;

/// Refusals a retry can fix. Everything else Slack refuses with
/// (`missing_scope`, `invalid_auth`, `account_inactive`, …) will say the same
/// thing next time.
const TRANSIENT: [&str; 4] = [
    "ratelimited",
    "internal_error",
    "fatal_error",
    "service_unavailable",
];

/// What `users.lookupByEmail` said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmailLookup {
    Found(LookupUser),
    /// No member of the workspace has this address.
    NotFound,
    /// Slack refused for a reason a retry will not change; the error code.
    Refused(String),
}

/// The fields of Slack's user object a caller decides on.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LookupUser {
    pub id: String,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub is_bot: bool,
    /// Set on a Slack Connect user from another organization.
    #[serde(default)]
    pub is_stranger: bool,
}

impl SlackClient {
    /// The workspace member with `email`. A transient refusal (rate limit,
    /// Slack's own fault) and a transport fault are `Err`, so a caller can
    /// retry them; see [`EmailLookup`] for the rest.
    pub async fn users_lookup_by_email(
        &self,
        bot_token: &str,
        email: &str,
    ) -> Result<EmailLookup, OxyError> {
        let url = format!("{}/users.lookupByEmail", self.base_url);
        let resp: Value = self
            .http
            .post(&url)
            .bearer_auth(bot_token)
            .form(&[("email", email)])
            .send()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("slack users.lookupByEmail http: {e}")))?
            .json()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("slack users.lookupByEmail json: {e}")))?;
        lookup_answer(resp)
    }
}

fn lookup_answer(resp: Value) -> Result<EmailLookup, OxyError> {
    if resp.get("ok").and_then(Value::as_bool) == Some(true) {
        let user = resp.get("user").cloned().unwrap_or(Value::Null);
        return serde_json::from_value(user)
            .map(EmailLookup::Found)
            .map_err(|e| OxyError::RuntimeError(format!("users.lookupByEmail decode: {e}")));
    }
    let code = resp
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    match code {
        "users_not_found" => Ok(EmailLookup::NotFound),
        c if TRANSIENT.contains(&c) => Err(OxyError::RuntimeError(format!(
            "slack users.lookupByEmail not ok: {c}"
        ))),
        c => Ok(EmailLookup::Refused(c.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_string_contains, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn answering(status: u16, body: Value) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/users.lookupByEmail"))
            .and(header("authorization", "Bearer xoxb-test"))
            .and(body_string_contains("email=ana%40example.com"))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .mount(&server)
            .await;
        server
    }

    async fn lookup(server: &MockServer) -> Result<EmailLookup, OxyError> {
        SlackClient::with_base_url(server.uri())
            .users_lookup_by_email("xoxb-test", "ana@example.com")
            .await
    }

    #[tokio::test]
    async fn a_member_is_found_with_the_fields_a_caller_decides_on() {
        let server = answering(
            200,
            serde_json::json!({ "ok": true, "user": { "id": "U1", "name": "ana", "is_bot": false } }),
        )
        .await;
        assert_eq!(
            lookup(&server).await.unwrap(),
            EmailLookup::Found(LookupUser {
                id: "U1".into(),
                deleted: false,
                is_bot: false,
                is_stranger: false,
            })
        );
    }

    #[tokio::test]
    async fn an_unknown_address_is_a_plain_no() {
        let server = answering(
            200,
            serde_json::json!({ "ok": false, "error": "users_not_found" }),
        )
        .await;
        assert_eq!(lookup(&server).await.unwrap(), EmailLookup::NotFound);
    }

    #[tokio::test]
    async fn a_rate_limit_is_an_error_so_the_caller_retries() {
        let server = answering(
            429,
            serde_json::json!({ "ok": false, "error": "ratelimited" }),
        )
        .await;
        assert!(lookup(&server).await.is_err());
    }

    #[tokio::test]
    async fn a_missing_scope_is_a_refusal_not_a_retry() {
        let server = answering(
            200,
            serde_json::json!({ "ok": false, "error": "missing_scope" }),
        )
        .await;
        assert_eq!(
            lookup(&server).await.unwrap(),
            EmailLookup::Refused("missing_scope".into())
        );
    }
}
