//! See: https://docs.aws.amazon.com/elasticloadbalancing/latest/application/listener-authenticate-users.html
//! See: https://github.com/awslabs/aws-jwt-verify/blob/main/src/jwt-model.ts
//! See: https://github.com/awslabs/aws-jwt-verify/blob/ba3a3806653aba17dd090253df9320458d8932c4/src/alb-verifier.ts
// to avoid complexity, we trust the alb and do not verify the signature or verify the signer

use axum::http::StatusCode;

use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

use super::{authenticator::Authenticator, types::Identity};

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

pub struct CognitoAuthenticator;

impl Default for CognitoAuthenticator {
    fn default() -> Self {
        Self::new()
    }
}

impl CognitoAuthenticator {
    pub fn new() -> Self {
        CognitoAuthenticator
    }
}

impl Authenticator for CognitoAuthenticator {
    type Error = CognitoError;

    async fn authenticate(&self, header: &axum::http::HeaderMap) -> Result<Identity, Self::Error> {
        let token = self.extract_token(header)?;
        self.validate(&token)
    }
}

impl CognitoAuthenticator {
    fn extract_token(&self, header: &axum::http::HeaderMap) -> Result<String, CognitoError> {
        // Only support ALB header (X-Amzn-Oidc-Data)
        if let Some(token) = header.get("X-Amzn-Oidc-Data").and_then(|v| v.to_str().ok()) {
            return Ok(token.to_string());
        }

        Err(CognitoError::AuthError(
            "Missing authentication header".to_string(),
        ))
    }

    fn validate(&self, encoded_jwt: &str) -> Result<Identity, CognitoError> {
        // AWS ALB uses standard base64 encoding with padding (=) for JWT tokens,
        // but the jsonwebtoken crate expects URL-safe base64 without padding per JWT spec.
        // Since we trust the ALB and don't need signature verification, we manually
        // decode the payload using standard base64 to avoid padding issues.
        // and we dont use jsonwebtoken crate for this purpose.

        // Decode JWT payload without signature verification
        // Split the JWT into parts (header.payload.signature)
        let parts: Vec<&str> = encoded_jwt.split('.').collect();
        if parts.len() != 3 {
            return Err(CognitoError::AuthError("Invalid JWT format".to_string()));
        }

        let payload_b64 = parts[1];

        let payload_bytes = general_purpose::STANDARD
            .decode(payload_b64)
            .map_err(|e| CognitoError::AuthError(format!("Failed to decode JWT payload: {}", e)))?;

        let payload_json = String::from_utf8_lossy(&payload_bytes);
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
    fn test_alb_authentication_flow() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-amzn-oidc-data",
            "eyJ0eXAiOiJKV1QiLCJraWQiOiIzMWY1NjZjNi05ZmVmLTQ2MDUtOWE2ZC1kMWIwNTMyODBjNzMiLCJhbGciOiJFUzI1NiIsImlzcyI6Imh0dHBzOi8vY29nbml0by1pZHAudXMtd2VzdC0yLmFtYXpvbmF3cy5jb20vdXMtd2VzdC0yX3NoWTAxTWhvZCIsImNsaWVudCI6InUyMXF2YmdkcHVtMzY2NjJhcWd2OG5nZjciLCJzaWduZXIiOiJhcm46YXdzOmVsYXN0aWNsb2FkYmFsYW5jaW5nOnVzLXdlc3QtMjozMzU3ODg2NDgwMzE6bG9hZGJhbGFuY2VyL2FwcC9veHktZGV2LXVzLXdlc3QtMi1veHktYWxiLzcyNDJkYTEyYzZlNzMzOTAiLCJleHAiOjE3NDk3MjkwMDR9.eyJzdWIiOiIxODExZDMyMC05MGYxLTcwYmQtZjZlMC05NTM1Y2FjZjU1ZGYiLCJlbWFpbF92ZXJpZmllZCI6ImZhbHNlIiwiaWRlbnRpdGllcyI6Ilt7XCJkYXRlQ3JlYXRlZFwiOlwiMTc0OTYyMzU1MzY0NlwiLFwidXNlcklkXCI6XCIxMDUxNTA2MDk3NzA3MDMwMTk5OTJcIixcInByb3ZpZGVyTmFtZVwiOlwiR29vZ2xlXCIsXCJwcm92aWRlclR5cGVcIjpcIkdvb2dsZVwiLFwiaXNzdWVyXCI6bnVsbCxcInByaW1hcnlcIjpcInRydWVcIn1dIiwibmFtZSI6Ikx1b25nIFZvIiwiZW1haWwiOiJsdW9uZ0BoeXBlcnF1ZXJ5LmFpIiwidXNlcm5hbWUiOiJnb29nbGVfMTA1MTUwNjA5NzcwNzAzMDE5OTkyIiwiZXhwIjoxNzQ5NzI5MDA0LCJpc3MiOiJodHRwczovL2NvZ25pdG8taWRwLnVzLXdlc3QtMi5hbWF6b25hd3MuY29tL3VzLXdlc3QtMl9zaFkwMU1ob2QifQ==.-4iv0Zfkz70RT_rC9Va_lZNVJeJjUDBjBOFX6qSdSHFsXwlTb4WAfK7oy8mrUQy5ircdDQ2pShnBsSkcRdYeKA==".parse().unwrap(),
        );

        let authenticator = CognitoAuthenticator::new();
        let token = authenticator.extract_token(&headers).unwrap();
        let identity = authenticator.validate(&token).unwrap();

        assert_eq!(identity.email, "luong@hyperquery.ai");
        assert_eq!(identity.name, Some("Luong Vo".to_string()));
        assert_eq!(
            identity.idp_id,
            Some("1811d320-90f1-70bd-f6e0-9535cacf55df".to_string())
        );
    }

    #[test]
    fn test_payload_parsing() {
        let payload_json = r#"{
            "sub": "1811d320-90f1-70bd-f6e0-9535cacf55df",
            "name": "Luong Vo",
            "email": "luong@hyperquery.ai",
            "username": "google_105150609770703019992",
            "email_verified": "false",
            "identities": "[{\"dateCreated\":\"1749623553646\",\"userId\":\"105150609770703019992\",\"providerName\":\"Google\",\"providerType\":\"Google\",\"issuer\":null,\"primary\":\"true\"}]",
            "exp": 1749729004,
            "iss": "https://cognito-idp.us-west-2.amazonaws.com/us-west-2_shY01Mhod"
        }"#;

        let payload: CognitoPayload = serde_json::from_str(payload_json).unwrap();
        assert_eq!(payload.sub, "1811d320-90f1-70bd-f6e0-9535cacf55df");
        assert_eq!(payload.email, "luong@hyperquery.ai");
        assert_eq!(payload.name, Some("Luong Vo".to_string()));
        assert_eq!(
            payload.username,
            Some("google_105150609770703019992".to_string())
        );
    }
}
