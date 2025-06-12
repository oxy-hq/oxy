//! See: https://docs.aws.amazon.com/elasticloadbalancing/latest/application/listener-authenticate-users.html
//! See: https://github.com/awslabs/aws-jwt-verify/blob/main/src/jwt-model.ts
//! See: https://github.com/awslabs/aws-jwt-verify/blob/ba3a3806653aba17dd090253df9320458d8932c4/src/alb-verifier.ts
// to avoid complexity, we trust the alb and do not verify the signature or verify the signer

use axum::http::StatusCode;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

use super::{types::Identity, validator::Validator};

#[derive(Debug, Error)]
pub enum CognitoError {
    #[error("{0}")]
    AuthError(String),
}

impl From<CognitoError> for StatusCode {
    fn from(_val: CognitoError) -> Self {
        StatusCode::UNAUTHORIZED
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CognitoPayload {
    // Required fields
    pub sub: String,
    pub email: String,
    // Optional fields for building name
    pub name: Option<String>,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    #[serde(rename = "cognito:username")]
    pub cognito_username: Option<String>,
    pub username: Option<String>,
    // Optional profile picture
    pub picture: Option<String>,
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
        // Only support ALB header (X-Amzn-Oidc-Data)
        if let Some(token) = header.get("X-Amzn-Oidc-Data").and_then(|v| v.to_str().ok()) {
            return Ok(token.to_string());
        }

        Err(CognitoError::AuthError(
            "Missing authentication header".to_string(),
        ))
    }

    fn validate(&self, encoded_jwt: &str) -> Result<Identity, Self::Error> {
        // Create a validation config that skips signature verification since we trust the ALB
        let mut validation = Validation::new(Algorithm::ES256);
        validation.insecure_disable_signature_validation();

        // Since we trust the ALB, we don't need to validate standard JWT claims
        validation.required_spec_claims.clear();
        validation.validate_exp = false;
        validation.validate_nbf = false;

        // Use a dummy key since we're not validating the signature
        let decoding_key = DecodingKey::from_secret(&[]);

        let token =
            decode::<CognitoPayload>(encoded_jwt, &decoding_key, &validation).map_err(|e| {
                CognitoError::AuthError(format!(
                    "Failed to decode JWT: {}. Original token: {}",
                    e, encoded_jwt
                ))
            })?;

        let payload = token.claims;
        let email = payload.email;
        let name = payload.name.filter(|n| !n.is_empty()).or_else(|| {
            match (payload.given_name.as_ref(), payload.family_name.as_ref()) {
                (Some(first), Some(last)) => Some(format!("{} {}", first, last)),
                (Some(first), None) => Some(first.clone()),
                (None, Some(last)) => Some(last.clone()),
                _ => payload
                    .cognito_username
                    .clone()
                    .or_else(|| payload.username.clone()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn test_alb_authentication_flow() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-amzn-oidc-data",
            "eyJhbGciOiJFUzI1NiIsImtpZCI6IjEyMzQ1Njc4LTEyMzQtMTIzNC0xMjM0LTEyMzQ1Njc4OTAxMiIsInNpZ25lciI6ImFybjphd3M6ZWxhc3RpY2xvYWRiYWxhbmNpbmc6dXMtZWFzdC0xOjEyMzQ1Njc4OTAxMjpsb2FkYmFsYW5jZXIvYXBwL215LWFsYi81MGRjNmM0OTVjMGM5MTg4IiwiaXNzIjoiaHR0cHM6Ly9jb2duaXRvLWlkcC51cy1lYXN0LTEuYW1hem9uYXdzLmNvbS91cy1lYXN0LTFfQUJDMTIzREVGIiwiY2xpZW50IjoiNGFiY2RlZmdoaWprbG1ub3BxcnN0dXZ3eHl6IiwiZXhwIjoxNzE4MjgwMDAwfQ.eyJzdWIiOiIxMjM0NTY3OC0xMjM0LTEyMzQtMTIzNC0xMjM0NTY3ODkwMTIiLCJuYW1lIjoiSm9obiBEb2UiLCJlbWFpbCI6ImpvaG4uZG9lQGV4YW1wbGUuY29tIiwiZW1haWxfdmVyaWZpZWQiOnRydWUsInBob25lX251bWJlciI6IisxMjM0NTY3ODkwIiwiY3VzdG9tOmRlcGFydG1lbnQiOiJFbmdpbmVlcmluZyIsImN1c3RvbTpyb2xlIjoiU2VuaW9yIERldmVsb3BlciJ9.fake_signature_for_testing_purposes_only".parse().unwrap(),
        );

        let validator = CognitoValidator::new();
        let token = validator.extract_token(&headers).unwrap();
        let identity = validator.validate(&token).unwrap();

        assert_eq!(identity.email, "john.doe@example.com");
        assert_eq!(identity.name, Some("John Doe".to_string()));
        assert_eq!(
            identity.idp_id,
            Some("12345678-1234-1234-1234-123456789012".to_string())
        );
    }

    #[test]
    fn test_payload_parsing() {
        let payload_json = r#"{
            "sub": "12345678-1234-1234-1234-123456789012",
            "name": "John Doe",
            "email": "john.doe@example.com"
        }"#;

        let payload: CognitoPayload = serde_json::from_str(payload_json).unwrap();
        assert_eq!(payload.sub, "12345678-1234-1234-1234-123456789012");
        assert_eq!(payload.email, "john.doe@example.com");
        assert_eq!(payload.name, Some("John Doe".to_string()));
    }
}
