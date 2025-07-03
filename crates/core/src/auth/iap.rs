use axum::http::StatusCode;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, jwk::JwkSet};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

use crate::config::constants::{
    GCP_IAP_EMAIL_HEADER_KEY, GCP_IAP_HEADER_KEY, GCP_IAP_ISS, GCP_IAP_PUBLIC_JWT_KEY,
    GCP_IAP_SUB_HEADER_KEY,
};

use super::{authenticator::Authenticator, types::Identity};

#[derive(Debug, Error)]
pub enum JwtError {
    #[error("Invalid JWT token: {0}")]
    InvalidToken(String),
    #[error("Missing JWT token")]
    MissingToken,
    #[error("Failed to decode JWT: {0}")]
    DecodingError(String),
    #[error("Token validation failed: {0}")]
    ValidationError(String),
    #[error("Internal error. Failed to parse jwk key: {0}")]
    JwkParseError(String),
}

impl From<JwtError> for StatusCode {
    fn from(val: JwtError) -> Self {
        match val {
            JwtError::InvalidToken(_) => StatusCode::UNAUTHORIZED,
            JwtError::MissingToken => StatusCode::UNAUTHORIZED,
            JwtError::DecodingError(_) => StatusCode::BAD_REQUEST,
            JwtError::ValidationError(_) => StatusCode::FORBIDDEN,
            JwtError::JwkParseError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IapClaims {
    pub iss: String,
    pub aud: String,
    pub sub: String,
    pub email: String,
    pub iat: i64,
    pub exp: i64,
}

impl fmt::Display for IapClaims {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "IapClaims {{ email: {}, sub: {} }}",
            self.email, self.sub
        )
    }
}

pub struct IAPAuthenticator {
    audience: String,
    is_cloud_run: bool,
}

impl IAPAuthenticator {
    pub fn new(audience: String, is_cloud_run: bool) -> Self {
        IAPAuthenticator {
            audience,
            is_cloud_run,
        }
    }
}

impl Authenticator for IAPAuthenticator {
    type Error = JwtError;

    async fn authenticate(&self, header: &axum::http::HeaderMap) -> Result<Identity, Self::Error> {
        match self.is_cloud_run {
            false => {
                let token = self.extract_token(header)?;
                self.validate(&token)
            }
            true => {
                tracing::info!("Running in Cloud Run, skipping JWT validation");
                let sub = header
                    .get(GCP_IAP_SUB_HEADER_KEY)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.split(":").last())
                    .ok_or(JwtError::MissingToken)?;
                let email = header
                    .get(GCP_IAP_EMAIL_HEADER_KEY)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.split(":").last())
                    .ok_or(JwtError::MissingToken)?;
                Ok(Identity {
                    idp_id: Some(sub.to_string()),
                    email: email.to_string(),
                    name: None,
                    picture: None,
                })
            }
        }
    }
}

impl IAPAuthenticator {
    fn extract_token(&self, header: &axum::http::HeaderMap) -> Result<String, JwtError> {
        tracing::info!("Extracting JWT token from header {:?}", header);
        header
            .get(GCP_IAP_HEADER_KEY)
            .and_then(|v| v.to_str().ok())
            .map(String::from)
            .ok_or(JwtError::MissingToken)
    }

    fn validate(&self, value: &str) -> Result<Identity, JwtError> {
        tracing::info!("Validating JWT token: {}", value);
        let jwks: JwkSet = serde_json::from_str(GCP_IAP_PUBLIC_JWT_KEY)
            .map_err(|err| JwtError::JwkParseError(err.to_string()))?;

        let header = jsonwebtoken::decode_header(value)
            .map_err(|err| JwtError::InvalidToken(err.to_string()))?;

        let jwk = match header.kid {
            Some(kid) => {
                tracing::info!("JWT header kid: {}", kid);
                jwks.find(&kid)
                    .ok_or_else(|| JwtError::InvalidToken(format!("No JWK found for kid: {kid}")))
            }
            None => Err(JwtError::InvalidToken("JWT header has no kid".to_string())),
        }?;

        let decoding_key =
            DecodingKey::from_jwk(jwk).map_err(|err| JwtError::DecodingError(err.to_string()))?;
        let mut validation = Validation::new(Algorithm::ES256);
        validation.set_audience(&[&self.audience]);
        validation.set_issuer(&[GCP_IAP_ISS]);
        validation.set_required_spec_claims(&["iss", "aud", "sub", "exp"]);

        jsonwebtoken::decode::<IapClaims>(value, &decoding_key, &validation)
            .map_err(|e| JwtError::DecodingError(e.to_string()))
            .map(|data| {
                let claims = data.claims;
                Identity {
                    idp_id: Some(claims.sub),
                    email: claims.email,
                    name: None,
                    picture: None,
                }
            })
    }
}
