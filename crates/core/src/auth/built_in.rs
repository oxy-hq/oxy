use crate::{
    config::{
        auth::{ApiKeyAuth, Authentication},
        constants::{AUTHENTICATION_HEADER_KEY, AUTHENTICATION_SECRET_KEY},
    },
    errors::OxyError,
};

use super::{api_key::ApiKeyAuthenticator, authenticator::Authenticator, types::Identity};
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
    authentication: Option<Authentication>,
}

impl Default for BuiltInAuthenticator {
    fn default() -> Self {
        Self::new(None)
    }
}

impl BuiltInAuthenticator {
    pub fn new(authentication: Option<Authentication>) -> Self {
        Self { authentication }
    }
}

impl Authenticator for BuiltInAuthenticator {
    type Error = OxyError;

    async fn authenticate(&self, header: &axum::http::HeaderMap) -> Result<Identity, Self::Error> {
        match self.authentication {
            None => Ok(Identity {
                idp_id: None,
                picture: None,
                email: "guest@oxy.local".to_string(),
                name: Some("Guest".to_string()),
            }),
            Some(ref auth) => match try_api_authentication(header, auth.api_key.clone()).await {
                Some(identity) => {
                    tracing::info!(
                        "API key authentication successful for user: {}",
                        identity.email
                    );
                    Ok(identity)
                }
                None => {
                    let token = self.extract_token(header)?;
                    self.validate(&token)
                }
            },
        }
    }
}

impl BuiltInAuthenticator {
    fn extract_token(&self, header: &axum::http::HeaderMap) -> Result<String, OxyError> {
        if self.authentication.is_none() {
            return Ok("".to_string());
        }

        tracing::info!("Extracting JWT token from header {:?}", header);
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

async fn try_api_authentication(
    header: &axum::http::HeaderMap,
    configuration: Option<ApiKeyAuth>,
) -> Option<Identity> {
    match configuration {
        None => None,
        Some(api_key_auth) => {
            let authenticator = ApiKeyAuthenticator::from_config(api_key_auth);
            let rs = authenticator.authenticate(header).await;
            rs.ok()
        }
    }
}
