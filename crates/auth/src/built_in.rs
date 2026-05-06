use std::sync::atomic::{AtomicBool, Ordering};

use crate::constants::{AUTHENTICATION_HEADER_KEY, AUTHENTICATION_SECRET_KEY};
use oxy_shared::errors::OxyError;

use crate::{api_key_infra::authenticate_header, authenticator::Authenticator, types::Identity};
use jsonwebtoken::{DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    email: String,
    exp: usize,
    iat: usize,
}

/// Process-wide flag toggled by the host (typically `oxy-app` at startup,
/// after parsing the OxyConfig) to tell `BuiltInAuthenticator` whether any
/// auth provider is configured. Defaults to `false` so zero-config installs
/// keep working in guest mode without the host having to call this.
///
/// This indirection exists so `oxy-auth` does not depend on the `oxy` crate
/// (the parsed OxyConfig lives there). Host calls
/// [`set_auth_configured`] once after config load.
static AUTH_CONFIGURED: AtomicBool = AtomicBool::new(false);

/// Tell `BuiltInAuthenticator` whether at least one auth provider (Google,
/// Okta, magic link, …) is configured. Call once at startup from the host.
pub fn set_auth_configured(value: bool) {
    AUTH_CONFIGURED.store(value, Ordering::Relaxed);
}

fn auth_configured() -> bool {
    AUTH_CONFIGURED.load(Ordering::Relaxed)
}

pub struct BuiltInAuthenticator;

impl Default for BuiltInAuthenticator {
    fn default() -> Self {
        Self
    }
}

impl BuiltInAuthenticator {
    pub fn new() -> Self {
        Self
    }
}

impl Authenticator for BuiltInAuthenticator {
    type Error = OxyError;

    async fn authenticate(&self, header: &axum::http::HeaderMap) -> Result<Identity, Self::Error> {
        // Check if any authentication methods are configured.
        // If YES: enforce authentication.
        // If NO: use guest user (backward compatibility for zero-config local installs).
        if !auth_configured() {
            return Ok(Identity {
                picture: None,
                name: Some("Local User".to_string()),
                email: crate::user::LOCAL_GUEST_EMAIL.to_string(),
            });
        }

        match self.extract_token(header) {
            Ok(token) => match self.validate(&token) {
                Ok(identity) => return Ok(identity),
                Err(err) => tracing::debug!("JWT validation failed, will try API key: {}", err),
            },
            Err(err) => tracing::debug!("No JWT token extracted: {}", err),
        }

        // Fallback to X-API-Key header authentication.
        authenticate_header(header).await
    }
}

impl BuiltInAuthenticator {
    fn extract_token(&self, header: &axum::http::HeaderMap) -> Result<String, OxyError> {
        tracing::debug!("Extracting JWT token from header");
        header
            .get(AUTHENTICATION_HEADER_KEY)
            .and_then(|v| v.to_str().ok())
            .map(String::from)
            .ok_or(OxyError::AuthenticationError(
                "Missing or invalid authentication header".to_string(),
            ))
    }

    fn validate(&self, value: &str) -> Result<Identity, OxyError> {
        let token_data = decode::<Claims>(
            value,
            &DecodingKey::from_secret(AUTHENTICATION_SECRET_KEY.as_bytes()),
            &Validation::default(),
        )
        .map_err(|err| {
            tracing::error!("JWT validation failed: {}", err);
            OxyError::AuthenticationError(format!("Invalid JWT token: {err}"))
        })?;

        Ok(Identity {
            picture: None,
            name: None,
            email: token_data.claims.email,
        })
    }
}
