use oxy::config::constants::{AUTHENTICATION_HEADER_KEY, AUTHENTICATION_SECRET_KEY};
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

pub struct BuiltInAuthenticator {
    pub cloud: bool,
}

impl Default for BuiltInAuthenticator {
    fn default() -> Self {
        Self::new(false)
    }
}

impl BuiltInAuthenticator {
    pub fn new(cloud: bool) -> Self {
        Self { cloud }
    }
}

impl Authenticator for BuiltInAuthenticator {
    type Error = OxyError;

    async fn authenticate(&self, header: &axum::http::HeaderMap) -> Result<Identity, Self::Error> {
        // In non-cloud mode, check if any authentication methods are configured
        // If YES: enforce authentication (same as cloud mode)
        // If NO: use guest user (backward compatibility)
        if !self.cloud {
            let has_auth_configured = oxy::config::oxy::get_oxy_config()
                .ok()
                .and_then(|config| config.authentication)
                .map(|auth| auth.basic.is_some() || auth.google.is_some() || auth.okta.is_some())
                .unwrap_or(false);

            if !has_auth_configured {
                return Ok(Identity {
                    idp_id: Some("local-user".to_string()),
                    picture: None,
                    name: Some("Local User".to_string()),
                    email: "<local-user@example.com>".to_string(),
                });
            }
        }

        // Enforce authentication (for both cloud mode and non-cloud with auth configured)
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
            idp_id: Some(token_data.claims.sub),
            picture: None,
            name: None,
            email: token_data.claims.email,
        })
    }
}
