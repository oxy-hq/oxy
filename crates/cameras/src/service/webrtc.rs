//! WebRTC live-preview session helper.
//!
//! Mints the per-session data an operator browser needs to establish
//! a WebRTC peer connection to the edge box's MediaMTX directly:
//!
//!   - the **WHEP URL** to POST the SDP offer at (per-box Tailscale
//!     Funnel hostname)
//!   - the **ICE servers** to configure on `RTCPeerConnection`,
//!     including a short-lived HMAC-derived TURN credential bound
//!     to the same Funnel hostname's `:8443` TCP port. Tailscale
//!     Funnel only permits public ports 443 / 8443 / 10000, so 8443
//!     is the TURN slot; Tailscale TLS-terminates and forwards
//!     plain TCP to coturn (see `video-poc/central/tailscale/funnel-init.sh`).
//!
//! Oxy is in the signaling path only (this helper computes URLs +
//! credentials; the route layer just hands the result to the
//! browser). Media flows browser ⇆ Funnel ⇆ box, never through
//! Oxy.

use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use sea_orm::{DatabaseConnection, EntityTrait};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use uuid::Uuid;

use crate::entities::{cameras, edge_boxes, sites};

use super::{ServiceError, ServiceResult};

/// One ICE server entry — same shape browsers feed to
/// `new RTCPeerConnection({ iceServers: [...] })`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceServer {
    /// `urls` must be a single-element list of one URL each. We
    /// keep the field as `Vec` because that's the RTC spec's shape
    /// and the browser will accept either a string or an array.
    pub urls: Vec<String>,
    pub username: String,
    pub credential: String,
}

/// Returned to the browser per session. Both the WHEP URL and the
/// TURN URL inside `ice_servers` point at the same per-box Funnel
/// hostname; the credentials are valid for `ttl` seconds from now.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebrtcSession {
    pub whep_url: String,
    pub ice_servers: Vec<IceServer>,
    pub expires_at: DateTime<Utc>,
}

/// TTL on the minted TURN credential. coturn's REST-API mode
/// requires the username to embed an absolute expiry timestamp, so
/// once the cred is handed out it's valid until that wall-clock
/// time regardless of session lifetime.
///
/// 5 minutes is plenty for one WebRTC negotiation + initial media
/// flow; ICE re-checks after that use the existing peer-connection
/// state and don't re-allocate. Operators who keep a tab open for
/// hours don't need a long TTL — the relay allocation already
/// established stays alive on coturn's side.
const TURN_CREDENTIAL_TTL_SECS: u64 = 5 * 60;

/// Build the per-session WebRTC data for one camera.
///
/// Workspace ownership chain matches `preview::recording_clip`:
/// camera → site → workspace, then edge_box binding check. Returns
/// 503-mapped errors when the box's `funnel_hostname` hasn't been
/// stamped yet (the edge worker hasn't checked in with the
/// `X-Edge-Funnel-Hostname` header).
pub async fn build_session(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    camera_id: Uuid,
    turn_auth_secret: &[u8],
) -> ServiceResult<WebrtcSession> {
    let cam = cameras::Entity::find_by_id(camera_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;

    let site = sites::Entity::find_by_id(cam.site_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if site.workspace_id != workspace_id {
        return Err(ServiceError::Forbidden(
            "camera belongs to another workspace",
        ));
    }

    let edge_box_id = cam
        .edge_box_id
        .ok_or(ServiceError::Unavailable("camera not bound to an edge box"))?;
    let edge_box = edge_boxes::Entity::find_by_id(edge_box_id)
        .one(db)
        .await?
        .ok_or(ServiceError::Unavailable(
            "camera's edge box no longer exists",
        ))?;

    let funnel = edge_box.funnel_hostname.ok_or(ServiceError::Unavailable(
        "edge box has no funnel_hostname yet — worker has not reported one via X-Edge-Funnel-Hostname",
    ))?;

    let mtx_path = format!("cam-{}", camera_id.simple());

    // Mint a WHEP-path access token so MediaMTX's HTTP-auth
    // callback can reject any guessed cam-UUID — see
    // `validate_whep_token` for the validation half.
    let (whep_token, whep_expiry) =
        mint_whep_token(&mtx_path, turn_auth_secret, TURN_CREDENTIAL_TTL_SECS)?;
    let whep_url =
        format!("https://{funnel}/{mtx_path}/whep?token={whep_token}&expiry={whep_expiry}");

    let (username, credential, expires_at) =
        mint_turn_credential(camera_id, turn_auth_secret, TURN_CREDENTIAL_TTL_SECS)?;

    // TURN endpoint advertised to the browser. Three considerations
    // forced the port choice and transport shape:
    //
    //   1. Tailscale Funnel only allows public ports 443 / 8443 /
    //      10000. Historical TURN-over-TLS (5349) is *not* in that
    //      list, so the original "turns:funnel:5349" entry was
    //      pointing at a black hole — Funnel never published 5349,
    //      browsers got ICE-failed candidate gather.
    //   2. The Tailscale sidecar's `--tls-terminated-tcp=8443`
    //      handles the public TLS, so coturn behind it sees plain
    //      TCP TURN. The browser still uses the `turns:` scheme
    //      (TLS-wrapped) because the public hop is TLS.
    //   3. `transport=tcp` mirrors how WebRTC clients negotiate the
    //      TURN allocation — the relay-side traffic between coturn
    //      and MTX flows inside the docker network as UDP, but
    //      that's invisible to the browser.
    //
    // See `video-poc/central/tailscale/funnel-init.sh` and the
    // `coturn` service in `docker-compose.yml` for the
    // companion config.
    //
    // Single entry is intentional. The earlier shape advertised a
    // plain-`turn:` fallback alongside `turns:`, but Tailscale
    // Funnel only ever publishes the TLS-wrapped 8443 hop — the
    // plain entry pointed at a port Funnel didn't expose, browsers
    // tried it first, ICE gather stalled. See commit c79beeb for
    // the regression that motivated the cleanup; restoring a
    // second entry needs a real path through Funnel.
    let ice_servers = vec![IceServer {
        urls: vec![format!("turns:{funnel}:8443?transport=tcp")],
        username: username.clone(),
        credential: credential.clone(),
    }];

    Ok(WebrtcSession {
        whep_url,
        ice_servers,
        expires_at,
    })
}

/// Mint a coturn REST-API credential.
///
/// Format (coturn `--use-auth-secret` mode):
///   - username = `<expiry_unix_ts>:<arbitrary_id>`
///   - credential = base64(HMAC-SHA1(secret, username))
///
/// coturn re-derives the credential from the username on receipt
/// using its known `--static-auth-secret`. No round-trip to coturn
/// at mint time; coturn doesn't need to know what we're handing out.
///
/// The arbitrary id is the camera UUID — useful for matching coturn
/// audit logs to specific cameras when debugging.
fn mint_turn_credential(
    camera_id: Uuid,
    secret: &[u8],
    ttl_secs: u64,
) -> ServiceResult<(String, String, DateTime<Utc>)> {
    let expiry = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ServiceError::Upstream("system clock before epoch".into()))?
        .as_secs()
        + ttl_secs;

    let username = format!("{expiry}:cam-{}", camera_id.simple());

    let mut mac = Hmac::<Sha1>::new_from_slice(secret)
        .map_err(|e| ServiceError::Upstream(format!("hmac key init failed: {e}")))?;
    mac.update(username.as_bytes());
    let credential = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    let expires_at = DateTime::<Utc>::from_timestamp(expiry as i64, 0)
        .ok_or_else(|| ServiceError::Upstream("expiry timestamp overflowed".into()))?;

    Ok((username, credential, expires_at))
}

// ── WHEP-path access token ──────────────────────────────────────────────────
//
// Tailscale Funnel exposes MediaMTX's WHEP endpoint to the public
// internet — anyone who guesses or scrapes a `cam-<uuid>/whep` URL
// could otherwise pull a live camera feed. We bind every minted
// WHEP URL to a per-session HMAC-SHA256 token over (mtx_path,
// expiry); MTX is configured with `authMethod: http` and calls
// back to Oxy's `/control/webrtc-auth` endpoint, which re-derives
// the HMAC and rejects mismatches.
//
// Token format (URL query):
//   ?token=<hex(HMAC-SHA256(secret, "<mtx_path>:<expiry>"))>
//   &expiry=<unix-ts>
//
// Mismatched path or stale expiry → 401 from validator → MTX
// rejects the WebRTC read. Operators never see the token; it's
// transparent in the URL the frontend POSTs the SDP at.

/// Mint the token + expiry. The expiry is the unix-second wall
/// clock past which the token must be rejected; the validator
/// compares against `SystemTime::now()` so clock drift on the box
/// is the only risk (and the same drift would already break
/// TURN, since coturn validates timestamps the same way).
fn mint_whep_token(mtx_path: &str, secret: &[u8], ttl_secs: u64) -> ServiceResult<(String, u64)> {
    let expiry = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ServiceError::Upstream("system clock before epoch".into()))?
        .as_secs()
        + ttl_secs;
    let token = compute_whep_hmac(mtx_path, expiry, secret)?;
    Ok((token, expiry))
}

/// Validate a token presented in a WHEP request. Returns Ok(()) if
/// the token + expiry combination matches the secret-bound HMAC
/// for the given path and the expiry hasn't passed.
pub fn validate_whep_token(
    mtx_path: &str,
    token: &str,
    expiry: u64,
    secret: &[u8],
) -> Result<(), &'static str> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock before epoch")?
        .as_secs();
    if expiry < now {
        return Err("token expired");
    }
    let expected =
        compute_whep_hmac(mtx_path, expiry, secret).map_err(|_| "hmac compute failed")?;
    // Constant-time compare — token comes from user-controlled
    // query string and a byte-by-byte mismatch would leak the
    // prefix-length match via timing.
    if !constant_time_eq(token.as_bytes(), expected.as_bytes()) {
        return Err("token mismatch");
    }
    Ok(())
}

fn compute_whep_hmac(mtx_path: &str, expiry: u64, secret: &[u8]) -> ServiceResult<String> {
    use sha2::Sha256;

    let mut mac = Hmac::<Sha256>::new_from_slice(secret)
        .map_err(|e| ServiceError::Upstream(format!("hmac key init failed: {e}")))?;
    mac.update(mtx_path.as_bytes());
    mac.update(b":");
    mac.update(expiry.to_string().as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mint_turn_credential_is_deterministic_and_well_formed() {
        // Same inputs → same outputs (verifies the HMAC and the
        // username are pure-functional given a fixed clock; we
        // emulate "fixed clock" by minting twice in rapid succession
        // and asserting the username's expiry differs by at most a
        // few seconds).
        let cam = Uuid::nil();
        let secret = b"test-secret-do-not-use-in-prod";

        let (u1, c1, e1) = mint_turn_credential(cam, secret, 300).unwrap();
        let (u2, c2, e2) = mint_turn_credential(cam, secret, 300).unwrap();

        // Username shape: <expiry>:cam-<32-char-hex>
        let parts: Vec<&str> = u1.split(':').collect();
        assert_eq!(parts.len(), 2);
        assert!(parts[0].parse::<u64>().is_ok());
        assert!(parts[1].starts_with("cam-"));
        assert_eq!(parts[1].len(), "cam-".len() + 32);

        // Within the same second the two should match; allow a 2s
        // skew either way for slow test runners.
        assert!((e1 - e2).num_seconds().abs() <= 2);

        // Same username → same HMAC (sanity).
        if u1 == u2 {
            assert_eq!(c1, c2);
        }

        // Credential is base64 of 20 bytes (sha1 output = 20 bytes).
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&c1)
            .unwrap();
        assert_eq!(decoded.len(), 20);
    }

    #[test]
    fn whep_token_mint_then_validate_roundtrip() {
        let secret = b"test-secret-do-not-use-in-prod";
        let path = "cam-abcd1234";

        let (token, expiry) = mint_whep_token(path, secret, 60).unwrap();
        assert!(validate_whep_token(path, &token, expiry, secret).is_ok());

        // Wrong path → reject.
        assert!(validate_whep_token("cam-evil", &token, expiry, secret).is_err());

        // Wrong token → reject. Flip the first char to one it definitely
        // ISN'T — replacing with a fixed "0" was flaky ~1/16 of runs when the
        // freshly-minted token already started with '0' (then bad == token and
        // validation correctly succeeded, failing this assertion).
        let mut bad = token.clone();
        let repl = if bad.starts_with('0') { "1" } else { "0" };
        bad.replace_range(..1, repl);
        assert_ne!(bad, token, "tampered token must differ from the original");
        assert!(validate_whep_token(path, &bad, expiry, secret).is_err());

        // Stale expiry → reject (use a far-past expiry).
        assert!(validate_whep_token(path, &token, 1, secret).is_err());

        // Different secret → reject.
        assert!(validate_whep_token(path, &token, expiry, b"other-secret").is_err());
    }
}
