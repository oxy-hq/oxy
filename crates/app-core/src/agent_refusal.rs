//! The handler-level refusal of a **sandbox agent token** (`oxy_sbx_`) on a
//! write to an app's production or staging state (sandbox agent credential
//! design, decision 6 and §3.3, carried to every such write).
//!
//! A sandbox agent token is its minter, who is Oxy staff, seen through the
//! narrowest credential there is. The staff console's two platform doors
//! admit it — it must pass them to reach its own sandbox routes — and the
//! console's scope check is by org, not by app. So what keeps the token from
//! promoting, rolling back, unpublishing or deleting an app in a granted
//! app's org is the route allow-list (`middlewares::app_grant_scope`) and
//! nothing else: none of those operations reaches `oxy-authz` with an
//! environment, so the model never sees them.
//!
//! [`RefuseSandboxAgent`] is the second refusal. A handler that writes state
//! a sandbox is not — a channel pointer, the app row, its audience, its
//! publishers, its storage — takes it as an argument, and the token is
//! answered `403 sandbox_token_refused` before the handler runs. It travels
//! with the handler wherever the handler is mounted, and shares no code with
//! the allow-list: it reads the credential the request authenticated with and
//! nothing about the route.
//!
//! **Keyed on the credential kind alone.** A session, a legacy API key, a
//! personal token, a service-account token, a `ci` token and a publish token
//! pass through untouched, as does a request nothing authenticated: each
//! handler's own checks decide those exactly as before.

use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use oxy_auth::token::CredentialContext;
use oxy_auth::types::AuthenticatedUser;

use crate::agent_body::Refusal;

/// The refusal's machine-readable code: the one a channel publish answers.
pub const CODE: &str = "sandbox_token_refused";

const MESSAGE: &str = "a sandbox agent token writes only to a sandbox it created: this changes \
                       an app's production or staging state";

/// Taken by a handler that writes production or staging state of a custom
/// app. Extracting it refuses a sandbox agent token; see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefuseSandboxAgent;

/// Whether the request authenticated with a sandbox agent token: the
/// credential on the user, or the one the auth layer left beside it.
fn is_sandbox_agent(parts: &Parts) -> bool {
    let on_user = parts
        .extensions
        .get::<AuthenticatedUser>()
        .and_then(|user| user.credential.as_ref());
    let beside = parts.extensions.get::<CredentialContext>();
    on_user
        .into_iter()
        .chain(beside)
        .any(CredentialContext::is_sandbox_agent)
}

/// `403 sandbox_token_refused`, in the one body the token is refused with
/// (`custom_apps_agent_body`): the answer a refused channel publish has.
pub fn refusal() -> Response {
    Refusal::new(StatusCode::FORBIDDEN, CODE, MESSAGE).into_response()
}

impl<S: Send + Sync> FromRequestParts<S> for RefuseSandboxAgent {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if is_sandbox_agent(parts) {
            Err(refusal())
        } else {
            Ok(Self)
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::http::Request;
    use oxy_auth::token::StoredKind;
    use uuid::Uuid;

    use super::*;
    use crate::agent_fixture as fixture;

    fn parts() -> Parts {
        Request::new(()).into_parts().0
    }

    fn user(credential: Option<CredentialContext>) -> AuthenticatedUser {
        fixture::user(credential)
    }

    fn token(kind: StoredKind) -> CredentialContext {
        let mut credential =
            fixture::credential(Uuid::from_u128(0x70), Uuid::nil(), Uuid::nil(), Uuid::nil());
        credential.kind = kind;
        credential
    }

    async fn extracted(mut parts: Parts) -> Result<RefuseSandboxAgent, Response> {
        RefuseSandboxAgent::from_request_parts(&mut parts, &()).await
    }

    /// The token is refused wherever the auth layer left its credential: on
    /// the user, beside the user, or both.
    #[tokio::test]
    async fn a_sandbox_agent_token_is_refused_with_the_code() {
        let agent = token(StoredKind::SandboxAgent);
        let mut on_user = parts();
        on_user.extensions.insert(user(Some(agent.clone())));
        let mut beside = parts();
        beside.extensions.insert(user(None));
        beside.extensions.insert(agent);
        for parts in [on_user, beside] {
            let refused = extracted(parts).await.expect_err("refused");
            assert_eq!(refused.status(), StatusCode::FORBIDDEN);
            let bytes = axum::body::to_bytes(refused.into_body(), 4096)
                .await
                .expect("body");
            let body: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
            assert_eq!(body["code"], "sandbox_token_refused");
            assert_eq!(body["error"], "sandbox_token_refused");
            assert_eq!(body["message"], MESSAGE);
            assert_eq!(body.as_object().map(serde_json::Map::len), Some(3));
        }
    }

    /// Every other credential, a session, and a request nothing authenticated
    /// pass through: the refusal is keyed on this kind and no other.
    #[tokio::test]
    async fn no_other_credential_is_touched() {
        assert!(extracted(parts()).await.is_ok(), "nothing authenticated");
        let mut session = parts();
        session.extensions.insert(user(None));
        assert!(extracted(session).await.is_ok(), "a browser session");
        for kind in [
            StoredKind::Personal,
            StoredKind::LegacyKey,
            StoredKind::ServiceAccount,
            StoredKind::Ci,
        ] {
            let mut parts = parts();
            parts.extensions.insert(user(Some(token(kind))));
            parts.extensions.insert(token(kind));
            assert!(extracted(parts).await.is_ok(), "{kind:?}");
        }
    }
}
