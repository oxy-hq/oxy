use crate::{
    auth::iap::JwtError,
    config::{
        auth::Authentication,
        constants::{AUTHENTICATION_HEADER_KEY, AUTHENTICATION_SECRET_KEY},
    },
};

use super::{types::Identity, validator::Validator};
use jsonwebtoken::{DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    email: String,
    exp: usize,
    iat: usize,
}

pub struct BuildInValidator {
    authentication: Option<Authentication>,
}

impl Default for BuildInValidator {
    fn default() -> Self {
        Self::new(None)
    }
}

impl BuildInValidator {
    pub fn new(authentication: Option<Authentication>) -> Self {
        Self { authentication }
    }
}

impl Validator for BuildInValidator {
    type Error = JwtError;
    fn extract_token(&self, header: &axum::http::HeaderMap) -> Result<String, Self::Error> {
        if self.authentication.is_none() {
            return Ok("".to_string());
        }

        tracing::info!("Extracting JWT token from header {:?}", header);
        header
            .get(AUTHENTICATION_HEADER_KEY)
            .and_then(|v| v.to_str().ok())
            .map(String::from)
            .ok_or(JwtError::MissingToken)
    }

    fn validate(&self, value: &str) -> Result<Identity, Self::Error> {
        if self.authentication.is_none() {
            return Ok(Identity {
                idp_id: None,
                picture: None,
                email: "guest@oxy.local".to_string(),
                name: Some("Guest".to_string()),
            });
        }

        let token_data = decode::<Claims>(
            value,
            &DecodingKey::from_secret(AUTHENTICATION_SECRET_KEY.as_bytes()),
            &Validation::default(),
        )
        .map_err(|err| {
            tracing::error!("JWT validation failed: {}", err);
            JwtError::ValidationError(err.to_string())
        })?;

        Ok(Identity {
            idp_id: Some(token_data.claims.sub),
            picture: None,
            name: None,
            email: token_data.claims.email,
        })
    }
}
