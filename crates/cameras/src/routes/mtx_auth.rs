//! MediaMTX HTTP-auth callback endpoint.
//!
//! MTX is configured with `authMethod: http` + `authHTTPAddress`
//! pointing here, and POSTs every publish / read / playback request
//! to us for validation before serving it. We validate the per-
//! session WHEP token Oxy minted in `service::webrtc::build_session`
//! (the `?token=…&expiry=…` query string riding on each WHEP URL)
//! and reject anything else.
//!
//! ## Trust boundary
//!
//! This route is intentionally NOT gated by the device-token
//! middleware that protects the rest of `/control/*` — MTX itself
//! is the caller and has no bearer to present. The route is
//! protected by:
//!
//!   1. **Tailnet ACL**: only `tag:edge-box` can reach
//!      `tag:oxy-backend`, so a public-internet attacker can't hit
//!      this endpoint even by guessing the URL.
//!   2. **HMAC validation**: the token + expiry in the callback's
//!      `query` field must match an Oxy-minted HMAC over
//!      (mtx_path, expiry) using the same secret coturn uses
//!      (OXY_CAMERAS_TURN_AUTH_SECRET).
//!
//! ## Action allowlist
//!
//! MTX calls back for every action — `publish`, `read`, `playback`,
//! `api`, `metrics`, `pprof`. We only require a token for
//! `read` + `protocol=webrtc` (the Funnel-exposed surface).
//! Everything else is allowed unconditionally — the worker
//! publishing RTSP, Oxy proxying HLS, MTX's own API + metrics —
//! because the tailnet ACL already gates who can reach those
//! interfaces.

use axum::extract::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{Router, post};
use serde::Deserialize;

use crate::service::webrtc;

/// Env var holding the HMAC secret shared with the WHEP minting
/// path. Same secret as the TURN credential mint — we deliberately
/// reuse to keep the operator's env config short. Inputs differ
/// (TURN: `"<expiry>:<cam-id>"`; WHEP: `"<mtx_path>:<expiry>"`) so
/// there's no ambiguity at validation time.
const TURN_AUTH_SECRET_ENV: &str = "OXY_CAMERAS_TURN_AUTH_SECRET";

pub fn routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/control/mtx-auth", post(post_mtx_auth))
}

/// The body MTX POSTs on every action. Field names match MTX 1.x
/// (`bluenviron/mediamtx`). `query` is the raw `?foo=bar&baz=qux`
/// portion of the inbound URL, without the leading `?`.
///
/// String fields are `Option<String>` because MTX 1.18+ emits
/// JSON `null` for absent values — `#[serde(default)]` alone only
/// handles missing keys, not explicit nulls. A `null` `id` was
/// silently failing every playback auth in the field; this is
/// the fix.
#[derive(Debug, Deserialize)]
struct MtxAuthRequest {
    action: String,
    #[serde(default, deserialize_with = "string_or_null")]
    protocol: String,
    #[serde(default, deserialize_with = "string_or_null")]
    path: String,
    #[serde(default, deserialize_with = "string_or_null")]
    query: String,
    // Other fields (user, password, ip, id) are accepted but
    // ignored — we don't use bearer-style auth at the MTX layer.
    #[serde(default, rename = "user", deserialize_with = "string_or_null")]
    _user: String,
    #[serde(default, rename = "password", deserialize_with = "string_or_null")]
    _password: String,
    #[serde(default, rename = "ip", deserialize_with = "string_or_null")]
    _ip: String,
    #[serde(default, rename = "id", deserialize_with = "string_or_null")]
    _id: String,
}

/// Custom deserializer: accepts a JSON string OR null, treating
/// the latter as an empty string. MTX emits `"field": null` for
/// absent values, which `String` rejects and `#[serde(default)]`
/// can't rescue.
fn string_or_null<'de, D>(de: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(de)?.unwrap_or_default())
}

async fn post_mtx_auth(Json(req): Json<MtxAuthRequest>) -> Response {
    // Log every callback so a 401 in the field is debuggable. We
    // saw a playback-401 in May 2026 where the diagnosis was
    // "does the callback even fire?" — having this line meant we
    // could answer in 30 seconds.
    tracing::info!(
        action = %req.action,
        protocol = %req.protocol,
        path = %req.path,
        has_query = !req.query.is_empty(),
        "mtx-auth callback"
    );

    // Public WHEP reads are the only action that needs a token.
    let needs_token = req.action == "read" && req.protocol == "webrtc";
    if !needs_token {
        return StatusCode::OK.into_response();
    }

    let secret = std::env::var(TURN_AUTH_SECRET_ENV).unwrap_or_default();
    if secret.is_empty() {
        tracing::warn!("mtx-auth: refusing WebRTC read — OXY_CAMERAS_TURN_AUTH_SECRET is not set");
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let (token, expiry) = match parse_token_query(&req.query) {
        Some(t) => t,
        None => {
            tracing::info!(
                path = %req.path,
                "mtx-auth: rejected WebRTC read — missing or malformed token query"
            );
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };

    match webrtc::validate_whep_token(&req.path, &token, expiry, secret.as_bytes()) {
        Ok(()) => StatusCode::OK.into_response(),
        Err(reason) => {
            tracing::info!(
                path = %req.path,
                reason = %reason,
                "mtx-auth: rejected WebRTC read"
            );
            StatusCode::UNAUTHORIZED.into_response()
        }
    }
}

/// Pull `token` + `expiry` out of MTX's raw query string. Format:
/// `token=<hex>&expiry=<unix-ts>`. Tolerates the two fields being
/// in either order and ignores any extras.
fn parse_token_query(query: &str) -> Option<(String, u64)> {
    let mut token: Option<String> = None;
    let mut expiry: Option<u64> = None;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        match (parts.next(), parts.next()) {
            (Some("token"), Some(v)) => token = Some(v.to_string()),
            (Some("expiry"), Some(v)) => expiry = v.parse::<u64>().ok(),
            _ => {}
        }
    }
    match (token, expiry) {
        (Some(t), Some(e)) => Some((t, e)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_token_query_extracts_both_fields() {
        let q = "token=deadbeef&expiry=1717000000";
        assert_eq!(
            parse_token_query(q),
            Some(("deadbeef".to_string(), 1_717_000_000))
        );
    }

    #[test]
    fn parse_token_query_handles_reverse_order() {
        let q = "expiry=42&token=cafe";
        assert_eq!(parse_token_query(q), Some(("cafe".to_string(), 42)));
    }

    #[test]
    fn parse_token_query_handles_extra_fields() {
        let q = "foo=bar&token=x&junk=&expiry=99";
        assert_eq!(parse_token_query(q), Some(("x".to_string(), 99)));
    }

    #[test]
    fn parse_token_query_rejects_missing_fields() {
        assert!(parse_token_query("").is_none());
        assert!(parse_token_query("token=x").is_none());
        assert!(parse_token_query("expiry=42").is_none());
        assert!(parse_token_query("expiry=notanumber&token=x").is_none());
    }

    /// Regression: MTX 1.18+ emits `"id": null` for connections
    /// that don't have an id (the playback HTTP server is one). A
    /// plain `String` field returned a 422 → MTX treated as
    /// auth-rejected → every playback got 401 with no log entry.
    /// `string_or_null` is the fix.
    #[test]
    fn deserialize_accepts_null_optional_fields() {
        let body = serde_json::json!({
            "action": "playback",
            "protocol": null,
            "path": "cam-x",
            "query": null,
            "user": "oxy",
            "password": null,
            "ip": "127.0.0.1",
            "id": null
        });
        let req: MtxAuthRequest = serde_json::from_value(body).expect("must accept null fields");
        assert_eq!(req.action, "playback");
        assert_eq!(req.protocol, "");
        assert_eq!(req.path, "cam-x");
        assert_eq!(req._user, "oxy");
        assert_eq!(req._id, "");
    }

    /// Belt-and-suspenders: a real MTX-shape body with all strings
    /// populated still parses.
    #[test]
    fn deserialize_accepts_all_strings_populated() {
        let body = serde_json::json!({
            "action": "read",
            "protocol": "webrtc",
            "path": "cam-x",
            "query": "token=abc&expiry=99",
            "user": "oxy",
            "password": "any",
            "ip": "100.64.1.42",
            "id": "conn-1"
        });
        let req: MtxAuthRequest = serde_json::from_value(body).unwrap();
        assert_eq!(req.action, "read");
        assert_eq!(req.protocol, "webrtc");
        assert_eq!(req._id, "conn-1");
    }
}
