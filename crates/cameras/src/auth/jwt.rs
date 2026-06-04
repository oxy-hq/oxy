//! Edge-device JWT auth (IoT Phase 3).
//!
//! Per-device JWTs replace the static `edge_box_tokens` bearer for
//! `/control/*`. Each device mints a short-lived (1h) token using
//! the HMAC-SHA256 secret already provisioned at factory enroll
//! (`device_registry.device_secret`) and sends it in the same
//! `Authorization: Bearer …` header. The server detects the
//! three-segment shape and routes to verification instead of the
//! bearer lookup.
//!
//! ## Wire shape
//!
//! ```text
//!   base64url(header) "." base64url(payload) "." base64url(sig)
//! ```
//!
//! ### Header
//!
//! `{"alg":"HS256","typ":"JWT"}` — the only header we accept. The
//! `none` algorithm is rejected with extreme prejudice; any non-HS256
//! alg is also rejected (no algorithm-confusion footgun).
//!
//! ### Payload
//!
//! ```json
//! { "iss": "<device_id_uuid>",
//!   "aud": "oxy-control",
//!   "exp": <unix_seconds> }
//! ```
//!
//! We deliberately do NOT include `edge_box_id` in the payload —
//! the server resolves it via `device_claims` on every request, so
//! a revoke + re-claim flips authorization immediately without
//! waiting for the device's cached token to expire.
//!
//! ## Why not a third-party JWT crate?
//!
//! `jsonwebtoken` ships ~250 lines of validation logic and pulls in
//! a meaningful dep tree. Our spec is tightly constrained (one alg,
//! three claims, no kid/jku/jwk surface) — a hand-rolled verifier is
//! ~60 lines and has zero algorithm-confusion attack surface. We
//! revisit if the spec grows.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use uuid::Uuid;

/// Acceptable wall-clock skew between device and server when
/// checking `exp`. Devices that NTP-sync drift by seconds, not
/// minutes; 60s is the safe-side window.
pub const EXP_SKEW_SECS: i64 = 60;

/// Canonical audience string. Devices and the server both sign
/// against this constant — a typo would cause silent failure on
/// upgrade.
pub const EXPECTED_AUD: &str = "oxy-control";

/// What we extract from a verified JWT. The caller still has to
/// resolve the device's active claim (`device_claims`) into a
/// concrete `edge_box_id` — that's a DB lookup that happens in the
/// middleware, not in this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedClaims {
    pub device_id: Uuid,
    pub exp: i64,
}

/// Why a JWT was rejected. Mapped to 401 by the middleware; the
/// variant is meant for tracing logs, not for the response body
/// (we don't want to give an attacker a why-it-failed oracle).
#[derive(Debug, thiserror::Error)]
pub enum JwtError {
    #[error("malformed: {0}")]
    Malformed(&'static str),
    #[error("unsupported alg: {0}")]
    UnsupportedAlg(String),
    #[error("bad signature")]
    BadSignature,
    #[error("expired")]
    Expired,
    #[error("wrong audience")]
    WrongAudience,
    #[error("malformed iss: {0}")]
    BadIssuer(&'static str),
}

/// Verify a JWT against a known device secret. Caller is
/// responsible for fetching the secret by `iss` before calling
/// (we need it in scope to verify the HMAC; the alternative is a
/// closure-based key lookup which is harder to test).
///
/// Returns the verified claims on success. Any failure is opaque
/// — callers should not branch on the variant for response shape.
pub fn verify(
    token: &str,
    device_secret: &[u8],
    now_secs: i64,
) -> Result<VerifiedClaims, JwtError> {
    // Three segments joined by `.`. We split into exactly three;
    // anything else is malformed (a fourth segment would let an
    // attacker tack on data after the signature).
    let mut parts = token.split('.');
    let header_b64 = parts.next().ok_or(JwtError::Malformed("no header"))?;
    let payload_b64 = parts.next().ok_or(JwtError::Malformed("no payload"))?;
    let sig_b64 = parts.next().ok_or(JwtError::Malformed("no signature"))?;
    if parts.next().is_some() {
        return Err(JwtError::Malformed("extra segments"));
    }

    // Header: only HS256 is accepted. We reject the `none` alg
    // explicitly (a famous footgun) and any algorithm we don't
    // implement — never trust the header's choice of crypto.
    let header_bytes = URL_SAFE_NO_PAD
        .decode(header_b64)
        .map_err(|_| JwtError::Malformed("header b64"))?;
    let header: JwtHeader =
        serde_json::from_slice(&header_bytes).map_err(|_| JwtError::Malformed("header json"))?;
    if header.alg != "HS256" {
        return Err(JwtError::UnsupportedAlg(header.alg));
    }

    // Signature: HMAC-SHA256 over `header_b64.payload_b64` (the
    // raw bytes, not the decoded ones — that's the spec).
    let signed = format!("{header_b64}.{payload_b64}");
    let sig_bytes = URL_SAFE_NO_PAD
        .decode(sig_b64)
        .map_err(|_| JwtError::Malformed("sig b64"))?;
    let mut mac = Hmac::<Sha256>::new_from_slice(device_secret)
        .map_err(|_| JwtError::Malformed("hmac key"))?;
    mac.update(signed.as_bytes());
    mac.verify_slice(&sig_bytes)
        .map_err(|_| JwtError::BadSignature)?;

    // Payload: parse only after sig has verified. Untrusted-input
    // discipline — we never look at attacker-controlled bytes
    // before crypto checks them.
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|_| JwtError::Malformed("payload b64"))?;
    let payload: JwtPayload =
        serde_json::from_slice(&payload_bytes).map_err(|_| JwtError::Malformed("payload json"))?;

    if payload.aud != EXPECTED_AUD {
        return Err(JwtError::WrongAudience);
    }
    if payload.exp + EXP_SKEW_SECS < now_secs {
        return Err(JwtError::Expired);
    }
    let device_id = Uuid::parse_str(&payload.iss).map_err(|_| JwtError::BadIssuer("not a uuid"))?;
    Ok(VerifiedClaims {
        device_id,
        exp: payload.exp,
    })
}

/// Sign a JWT with the given secret. Production code path lives
/// on the device (Python), so this is here for tests / future CLI
/// tools rather than for use in the request hot path.
pub fn sign(device_id: Uuid, device_secret: &[u8], exp_secs: i64) -> String {
    let header_json = r#"{"alg":"HS256","typ":"JWT"}"#;
    let payload = JwtPayload {
        iss: device_id.to_string(),
        aud: EXPECTED_AUD.to_string(),
        exp: exp_secs,
    };
    let payload_json = serde_json::to_string(&payload).expect("payload always serializes");
    let header_b64 = URL_SAFE_NO_PAD.encode(header_json.as_bytes());
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
    let signed = format!("{header_b64}.{payload_b64}");
    let mut mac = Hmac::<Sha256>::new_from_slice(device_secret).expect("hmac key length checked");
    mac.update(signed.as_bytes());
    let sig = mac.finalize().into_bytes();
    let sig_b64 = URL_SAFE_NO_PAD.encode(sig);
    format!("{header_b64}.{payload_b64}.{sig_b64}")
}

/// Loose JWT-shape check used by the middleware to decide which
/// path (JWT vs legacy bearer) to enter. `true` means "looks like
/// a JWT, try verify"; `false` means "definitely not a JWT, do
/// bearer lookup." Cheap — no allocation, no base64 decode.
///
/// We require all three segments to be non-empty and consist only
/// of base64url chars (letters, digits, `-`, `_`). The bearer
/// tokens minted by [`crate::auth::token::mint`] are random
/// base64 of a fixed length and never contain dots, so the two
/// shapes are unambiguous on the wire.
pub fn looks_like_jwt(s: &str) -> bool {
    let mut count = 0usize;
    let mut current_empty = true;
    for c in s.chars() {
        if c == '.' {
            if current_empty {
                return false;
            }
            count += 1;
            current_empty = true;
            if count > 2 {
                return false;
            }
        } else if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            current_empty = false;
        } else {
            return false;
        }
    }
    count == 2 && !current_empty
}

#[derive(Debug, serde::Deserialize)]
struct JwtHeader {
    alg: String,
    // typ is optional per RFC 7519; we don't enforce.
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct JwtPayload {
    iss: String,
    aud: String,
    exp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_secret() -> Vec<u8> {
        // 32 bytes — matches the production
        // `service::fleet::generate_device_secret` length so we're
        // testing the actual hot path.
        (0..32u8).collect()
    }

    #[test]
    fn looks_like_jwt_accepts_three_b64_segments() {
        assert!(looks_like_jwt("aaa.bbb.ccc"));
        assert!(looks_like_jwt("a-b.c_d.e1f"));
    }

    #[test]
    fn looks_like_jwt_rejects_bearer_shape() {
        // Production bearer tokens are flat base64 with no dots.
        assert!(!looks_like_jwt("plainbearer123"));
        assert!(!looks_like_jwt("AAAA-BBBB"));
    }

    #[test]
    fn looks_like_jwt_rejects_wrong_segment_count() {
        assert!(!looks_like_jwt("aaa"));
        assert!(!looks_like_jwt("aaa.bbb"));
        assert!(!looks_like_jwt("aaa.bbb.ccc.ddd"));
    }

    #[test]
    fn looks_like_jwt_rejects_empty_segment() {
        assert!(!looks_like_jwt("aaa..ccc"));
        assert!(!looks_like_jwt("aaa.bbb."));
        assert!(!looks_like_jwt(".bbb.ccc"));
    }

    #[test]
    fn sign_and_verify_round_trip() {
        let secret = fixed_secret();
        let device_id = Uuid::new_v4();
        let now = 1_700_000_000_i64;
        let token = sign(device_id, &secret, now + 3600);
        let v = verify(&token, &secret, now).expect("clean verify");
        assert_eq!(v.device_id, device_id);
        assert_eq!(v.exp, now + 3600);
    }

    #[test]
    fn verify_rejects_expired_token() {
        let secret = fixed_secret();
        let device_id = Uuid::new_v4();
        let now = 1_700_000_000_i64;
        let token = sign(device_id, &secret, now - 1_000);
        let err = verify(&token, &secret, now).unwrap_err();
        assert!(matches!(err, JwtError::Expired));
    }

    #[test]
    fn verify_accepts_within_skew() {
        let secret = fixed_secret();
        let device_id = Uuid::new_v4();
        let now = 1_700_000_000_i64;
        // exp 30s in the past, within the 60s skew window.
        let token = sign(device_id, &secret, now - 30);
        verify(&token, &secret, now).expect("within skew");
    }

    #[test]
    fn verify_rejects_bad_signature() {
        let secret = fixed_secret();
        let other_secret: Vec<u8> = (32..64u8).collect();
        let device_id = Uuid::new_v4();
        let now = 1_700_000_000_i64;
        let token = sign(device_id, &other_secret, now + 3600);
        let err = verify(&token, &secret, now).unwrap_err();
        assert!(matches!(err, JwtError::BadSignature));
    }

    #[test]
    fn verify_rejects_none_alg() {
        // Manually craft a `none` header to confirm we never
        // accept it. The body must still parse as a JWT or we'd
        // exit on "malformed" before getting to the alg check.
        let header_b64 = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#.as_bytes());
        let payload_b64 = URL_SAFE_NO_PAD.encode(
            serde_json::to_string(&JwtPayload {
                iss: Uuid::new_v4().to_string(),
                aud: EXPECTED_AUD.into(),
                exp: 1_700_000_000 + 3600,
            })
            .unwrap()
            .as_bytes(),
        );
        let token = format!("{header_b64}.{payload_b64}.");
        let err = verify(&token, &fixed_secret(), 1_700_000_000).unwrap_err();
        assert!(matches!(err, JwtError::UnsupportedAlg(_)));
    }

    #[test]
    fn verify_rejects_wrong_audience() {
        let secret = fixed_secret();
        let device_id = Uuid::new_v4();
        let header_b64 = URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256","typ":"JWT"}"#.as_bytes());
        let payload_b64 = URL_SAFE_NO_PAD.encode(
            serde_json::to_string(&JwtPayload {
                iss: device_id.to_string(),
                aud: "other-service".into(),
                exp: 1_700_000_000 + 3600,
            })
            .unwrap()
            .as_bytes(),
        );
        let signed = format!("{header_b64}.{payload_b64}");
        let mut mac = Hmac::<Sha256>::new_from_slice(&secret).unwrap();
        mac.update(signed.as_bytes());
        let sig_b64 = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        let token = format!("{signed}.{sig_b64}");
        let err = verify(&token, &secret, 1_700_000_000).unwrap_err();
        assert!(matches!(err, JwtError::WrongAudience));
    }

    /// Regression vector — locks server-side verify against the
    /// Python worker's mint output. If either side changes its
    /// payload encoding (field order, separator whitespace, etc.)
    /// this fails before production sees the drift.
    ///
    /// The literal token was produced by running
    /// `worker/jwt_mint.py::_sign` against the same inputs.
    #[test]
    fn verify_accepts_python_worker_mint_output() {
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiIxMTExMTExMS0xMTExLTcwMDAtODAwMC0yMjIyMjIyMjIyMjIiLCJhdWQiOiJveHktY29udHJvbCIsImV4cCI6MTcwMDAwMzYwMH0.ZIKhblnoV9YUsVh9IHJpAHmnQpe_4ukOSRPq7cvQYQM";
        let secret: Vec<u8> = (0..32u8).collect();
        let now = 1_700_000_000_i64;
        let v = verify(token, &secret, now).expect("python-minted token verifies");
        assert_eq!(
            v.device_id,
            Uuid::parse_str("11111111-1111-7000-8000-222222222222").unwrap()
        );
        assert_eq!(v.exp, 1_700_000_000 + 3600);
    }

    #[test]
    fn verify_rejects_extra_segments() {
        let secret = fixed_secret();
        let device_id = Uuid::new_v4();
        let token = sign(device_id, &secret, 1_700_000_000 + 3600);
        let extra = format!("{token}.tacked-on");
        let err = verify(&extra, &secret, 1_700_000_000).unwrap_err();
        assert!(matches!(err, JwtError::Malformed(_)));
    }
}
