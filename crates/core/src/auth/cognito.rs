//! See: https://docs.aws.amazon.com/elasticloadbalancing/latest/application/listener-authenticate-users.html
//! See: https://github.com/awslabs/aws-jwt-verify/blob/main/src/jwt-model.ts
// to avoid complexity, we trust the alb and do not verify the signature or verify the signer

use axum::http::StatusCode;
use base64::{Engine as _, engine::general_purpose};
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
        // Decode JWT payload without signature verification
        // Split the JWT into parts (header.payload.signature)
        let parts: Vec<&str> = encoded_jwt.split('.').collect();
        if parts.len() != 3 {
            return Err(CognitoError::AuthError("Invalid JWT format".to_string()));
        }

        // Decode the payload. JWT uses URL-safe base64 without padding
        let payload_b64 = parts[1];

        let payload_bytes = general_purpose::URL_SAFE_NO_PAD
            .decode(payload_b64)
            .map_err(|e| CognitoError::AuthError(format!("Failed to decode JWT payload: {}", e)))?;

        // Log the decoded payload for debugging
        let payload_json = String::from_utf8_lossy(&payload_bytes);
        log::debug!("Decoded JWT payload: {}", payload_json);

        let payload: CognitoPayload = serde_json::from_slice(&payload_bytes).map_err(|e| {
            CognitoError::AuthError(format!(
                "Failed to parse JWT payload: {}. Raw payload: {}",
                e, payload_json
            ))
        })?;

        // Extract user information
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
    fn test_extract_alb_header() {
        // Test extracting JWT token from AWS ALB header
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-amzn-oidc-data",
            "eyJhbGciOiJFUzI1NiIsImtpZCI6IjEyMzQ1Njc4LTEyMzQtMTIzNC0xMjM0LTEyMzQ1Njc4OTAxMiIsInNpZ25lciI6ImFybjphd3M6ZWxhc3RpY2xvYWRiYWxhbmNpbmc6dXMtZWFzdC0xOjEyMzQ1Njc4OTAxMjpsb2FkYmFsYW5jZXIvYXBwL215LWFsYi81MGRjNmM0OTVjMGM5MTg4IiwiaXNzIjoiaHR0cHM6Ly9jb2duaXRvLWlkcC51cy1lYXN0LTEuYW1hem9uYXdzLmNvbS91cy1lYXN0LTFfQUJDMTIzREVGIiwiY2xpZW50IjoiNGFiY2RlZmdoaWprbG1ub3BxcnN0dXZ3eHl6IiwiZXhwIjoxNzE4MjgwMDAwfQ.eyJzdWIiOiIxMjM0NTY3OC0xMjM0LTEyMzQtMTIzNC0xMjM0NTY3ODkwMTIiLCJuYW1lIjoiSm9obiBEb2UiLCJlbWFpbCI6ImpvaG4uZG9lQGV4YW1wbGUuY29tIiwiZW1haWxfdmVyaWZpZWQiOnRydWUsInBob25lX251bWJlciI6IisxMjM0NTY3ODkwIiwiY3VzdG9tOmRlcGFydG1lbnQiOiJFbmdpbmVlcmluZyIsImN1c3RvbTpyb2xlIjoiU2VuaW9yIERldmVsb3BlciJ9.fake_signature_for_testing_purposes_only".parse().unwrap(),
        );

        let validator = CognitoValidator::new();
        let result = validator.extract_token(&headers);

        assert!(result.is_ok());
        let token = result.unwrap();
        assert!(token.starts_with("eyJhbGciOiJFUzI1NiI"));
    }

    #[test]
    fn test_decode_jwt_payload_structure() {
        let payload_json = r#"{
            "sub": "12345678-1234-1234-1234-123456789012",
            "name": "John Doe", 
            "email": "john.doe@example.com",
            "email_verified": true,
            "phone_number": "+12345678901",
            "custom:department": "Engineering",
            "custom:role": "Senior Developer"
        }"#;

        let payload: CognitoPayload = serde_json::from_str(payload_json).unwrap();

        // Verify we extract the essential fields
        assert_eq!(payload.sub, "12345678-1234-1234-1234-123456789012");
        assert_eq!(payload.email, "john.doe@example.com");
        assert_eq!(payload.name, Some("John Doe".to_string()));
        assert_eq!(payload.picture, None);
    }

    #[test]
    fn test_complete_alb_authentication_flow() {
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
        assert_eq!(identity.picture, None);
    }
}
