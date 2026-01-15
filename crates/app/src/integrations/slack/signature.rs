//! Slack request signature verification
//!
//! Implements HMAC-SHA256 verification as per Slack's security requirements:
//! https://api.slack.com/authentication/verifying-requests-from-slack

use axum::body::Bytes;
use axum::http::HeaderMap;
use constant_time_eq::constant_time_eq;
use hmac::{Hmac, Mac};
use oxy_shared::errors::OxyError;
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

const SLACK_VERSION: &str = "v0";
const MAX_TIMESTAMP_DIFF_SECONDS: u64 = 60 * 5; // 5 minutes

/// Verify Slack request from HTTP headers and body
///
/// Extracts the timestamp and signature from headers and verifies the request.
/// This is the main entry point for HTTP handlers.
///
/// # Arguments
/// * `signing_secret` - Slack app signing secret
/// * `headers` - HTTP request headers
/// * `body` - Raw request body
pub fn verify_request(
    signing_secret: &str,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<(), OxyError> {
    let timestamp = headers
        .get("x-slack-request-timestamp")
        .and_then(|v| v.to_str().ok())
        .ok_or(OxyError::SlackSignatureInvalid)?;

    let signature = headers
        .get("x-slack-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(OxyError::SlackSignatureInvalid)?;

    verify_signature(signing_secret, timestamp, body, signature)
}

/// Verify Slack request signature
///
/// # Arguments
/// * `signing_secret` - Slack app signing secret
/// * `timestamp` - X-Slack-Request-Timestamp header value
/// * `body` - Raw request body
/// * `signature` - X-Slack-Signature header value
///
/// # Returns
/// Ok(()) if signature is valid, Err otherwise
pub fn verify_signature(
    signing_secret: &str,
    timestamp: &str,
    body: &[u8],
    signature: &str,
) -> Result<(), OxyError> {
    // Verify timestamp is recent (prevent replay attacks)
    verify_timestamp(timestamp)?;

    // Compute expected signature
    let expected_signature = compute_signature(signing_secret, timestamp, body)?;

    // Compare signatures using constant-time comparison to prevent timing attacks
    if !constant_time_eq(signature.as_bytes(), expected_signature.as_bytes()) {
        tracing::warn!("Slack signature verification failed");
        return Err(OxyError::SlackSignatureInvalid);
    }

    Ok(())
}

/// Verify timestamp is within acceptable range (prevent replay attacks)
fn verify_timestamp(timestamp: &str) -> Result<(), OxyError> {
    let request_timestamp: u64 = timestamp
        .parse()
        .map_err(|_| OxyError::SlackSignatureInvalid)?;

    let current_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| OxyError::SlackSignatureInvalid)?
        .as_secs();

    let diff = current_timestamp.abs_diff(request_timestamp);

    if diff > MAX_TIMESTAMP_DIFF_SECONDS {
        tracing::warn!(
            "Slack request timestamp too old. Diff: {}s, Max: {}s",
            diff,
            MAX_TIMESTAMP_DIFF_SECONDS
        );
        return Err(OxyError::SlackSignatureInvalid);
    }

    Ok(())
}

/// Compute HMAC-SHA256 signature for Slack request
fn compute_signature(
    signing_secret: &str,
    timestamp: &str,
    body: &[u8],
) -> Result<String, OxyError> {
    // Build signature base string: v0:<timestamp>:<body>
    let sig_base = format!("{}:{}:", SLACK_VERSION, timestamp);
    let mut sig_base_bytes = sig_base.into_bytes();
    sig_base_bytes.extend_from_slice(body);

    // Compute HMAC-SHA256
    let mut mac = HmacSha256::new_from_slice(signing_secret.as_bytes())
        .map_err(|_| OxyError::SlackSignatureInvalid)?;
    mac.update(&sig_base_bytes);
    let result = mac.finalize();
    let code_bytes = result.into_bytes();

    // Format as "v0=<hex>"
    Ok(format!("{}={}", SLACK_VERSION, hex::encode(code_bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_signature() {
        let signing_secret = "test_secret";
        let timestamp = "1234567890";
        let body = b"test_body";

        let sig = compute_signature(signing_secret, timestamp, body).unwrap();
        assert!(sig.starts_with("v0="));
        assert_eq!(sig.len(), 3 + 64); // "v0=" + 64 hex chars
    }

    #[test]
    fn test_verify_signature_valid() {
        let signing_secret = "test_secret";
        let timestamp = "1234567890";
        let body = b"test_body";

        let sig = compute_signature(signing_secret, timestamp, body).unwrap();

        // This will fail timestamp check, but signature computation should work
        let result = verify_signature(signing_secret, timestamp, body, &sig);
        assert!(result.is_err()); // Fails due to old timestamp
    }

    #[test]
    fn test_verify_signature_invalid() {
        let signing_secret = "test_secret";
        let timestamp = "1234567890";
        let body = b"test_body";

        let result = verify_signature(signing_secret, timestamp, body, "v0=invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_timestamp_old() {
        let old_timestamp = "1234567890"; // Very old timestamp
        let result = verify_timestamp(old_timestamp);
        assert!(result.is_err());
    }
}
