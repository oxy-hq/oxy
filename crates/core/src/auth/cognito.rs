use axum::http::StatusCode;
use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

use super::{types::Identity, validator::Validator};

#[derive(Debug, Error)]
pub enum CognitoError {
    #[error("Invalid JWT token: {0}")]
    InvalidToken(String),
    #[error("Missing auth header")]
    MissingAuthHeader,
    #[error("Base64 decode error: {0}")]
    Base64Error(String),
    #[error("JSON parse error: {0}")]
    JsonError(String),
    #[error("Email not found in payload")]
    EmailMissing,
}

impl From<CognitoError> for StatusCode {
    fn from(val: CognitoError) -> Self {
        match val {
            CognitoError::InvalidToken(_) => StatusCode::UNAUTHORIZED,
            CognitoError::MissingAuthHeader => StatusCode::UNAUTHORIZED,
            CognitoError::Base64Error(_) => StatusCode::BAD_REQUEST,
            CognitoError::JsonError(_) => StatusCode::BAD_REQUEST,
            CognitoError::EmailMissing => StatusCode::BAD_REQUEST,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CognitoPayload {
    pub sub: String,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub cognito_username: Option<String>,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub name: Option<String>,
    pub picture: Option<String>,
    // Optional fields that may or may not be present
    pub aud: Option<String>,
    pub iss: Option<String>,
    pub token_use: Option<String>,
    pub auth_time: Option<i64>,
    pub exp: Option<i64>,
    pub iat: Option<i64>,
}

impl fmt::Display for CognitoPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CognitoPayload {{ email: {:?}, sub: {} }}",
            self.email, self.sub
        )
    }
}

pub struct CognitoValidator;

impl Default for CognitoValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl CognitoValidator {
    pub fn new() -> Self {
        CognitoValidator
    }
}

impl Validator for CognitoValidator {
    type Error = CognitoError;

    fn extract_token(&self, header: &axum::http::HeaderMap) -> Result<String, Self::Error> {
        // Try ALB header first (X-Amzn-Oidc-Data)
        if let Some(token) = header.get("X-Amzn-Oidc-Data").and_then(|v| v.to_str().ok()) {
            return Ok(token.to_string());
        }

        // Fall back to Authorization header for direct Cognito
        if let Some(auth_header) = header.get("Authorization").and_then(|v| v.to_str().ok()) {
            if let Some(token) = auth_header.strip_prefix("Bearer ") {
                return Ok(token.to_string());
            }
        }

        Err(CognitoError::MissingAuthHeader)
    }

    fn validate(&self, encoded_jwt: &str) -> Result<Identity, Self::Error> {
        let jwt_parts: Vec<&str> = encoded_jwt.split('.').collect();
        if jwt_parts.len() != 3 {
            return Err(CognitoError::InvalidToken("Invalid JWT format".to_string()));
        }

        // Decode payload directly (trust ALB/Cognito has already validated)
        let payload_part = jwt_parts[1];
        let decoded_payload = general_purpose::URL_SAFE_NO_PAD
            .decode(payload_part)
            .or_else(|_| general_purpose::STANDARD_NO_PAD.decode(payload_part))
            .map_err(|e| CognitoError::Base64Error(e.to_string()))?;

        let payload_str = String::from_utf8(decoded_payload)
            .map_err(|e| CognitoError::Base64Error(e.to_string()))?;

        let payload: CognitoPayload = serde_json::from_str(&payload_str)
            .map_err(|e| CognitoError::JsonError(e.to_string()))?;

        let email = payload.email.ok_or(CognitoError::EmailMissing)?;

        let name = payload.name.or_else(|| {
            match (payload.given_name.as_ref(), payload.family_name.as_ref()) {
                (Some(first), Some(last)) => Some(format!("{} {}", first, last)),
                (Some(first), None) => Some(first.clone()),
                (None, Some(last)) => Some(last.clone()),
                _ => payload.cognito_username.clone(),
            }
        });

        Ok(Identity {
            idp_id: Some(payload.sub),
            email,
            name,
            picture: payload.picture,
        })
    }
}
