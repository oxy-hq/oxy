//! Axum middleware that authenticates an inbound `/control/*` request
//! via the `Authorization: Bearer <jwt>` header and injects an
//! [`EdgeContext`] into `request.extensions` for downstream handlers.
//!
//! Mounted as `axum::middleware::from_fn_with_state(db.clone(),
//! require_device_token)` on the `/control/*` router subtree.
//!
//! All edge auth goes through per-device JWTs minted from a sealed
//! `device_secret` in `device_registry`.
//!
//! Failure modes (all 401):
//!   - missing or malformed `Authorization: Bearer …` header
//!   - JWT signature mismatch, expired, or `iss` not in `device_registry`
//!   - `device_claims` has no row with `status='claimed'` for this device
//!     (operator revoked the claim or never finished bootstrap)
//!   - edge_box / site lookup fails (claim points at a deleted box)
//!   - edge_box is retired (operator clicked Remove)
//!
//! Side effects, all best-effort (a failure logs but never blocks the
//! request):
//!   - `edge_boxes.status` flips to `active` on first successful auth
//!   - `edge_boxes.tailscale_ip` is auto-tracked from the request's
//!     reachable address so snapshot / HLS proxies can dial back
//!   - `edge_boxes.funnel_hostname` is recorded from the
//!     `X-Edge-Funnel-Hostname` header for WebRTC session URL minting
//!   - `edge_boxes.last_seen_at` is stamped on every call

use axum::{
    body::Body,
    http::{Request, StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::Response,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

use crate::auth::context::EdgeContext;
use crate::auth::jwt;
use crate::entities::{cameras, device_claims, device_registry, edge_boxes, sites};

/// Middleware that authenticates an edge box via per-device JWT and
/// injects [`EdgeContext`] into request extensions.
///
/// Reads the `DatabaseConnection` from request extensions (mounted by
/// the router via `.layer(Extension(db.clone()))`). This lets the
/// router be generic over caller state, matching the agentic-http
/// pattern.
pub async fn require_device_token(
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let db = req
        .extensions()
        .get::<DatabaseConnection>()
        .cloned()
        .ok_or_else(|| {
            tracing::error!(
                "edge-auth: DatabaseConnection extension missing — router not mounted correctly"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let bearer = extract_bearer(&req).ok_or(StatusCode::UNAUTHORIZED)?;

    if !jwt::looks_like_jwt(&bearer) {
        // A non-JWT credential here is either a stale device.json
        // from before the IoT identity layer or a misconfigured
        // client. 401 with a log so support can identify the wave.
        tracing::warn!("edge-auth: non-JWT credential rejected — device needs re-onboarding");
        return Err(StatusCode::UNAUTHORIZED);
    }

    let (edge_box, claim_id) = verify_jwt(&db, &bearer).await?;

    // Retired-box guard. The JWT path verifies against the device's
    // HMAC secret (which stays in `device_registry` even after retire)
    // and would otherwise keep authenticating forever. After retire,
    // the operator's intent is "this box is gone": send 401 so the
    // worker stops cleanly. Without this check the liveness batch
    // below was re-flipping `status` back to `active` on every poll —
    // operators saw retired boxes resurface within 30s of clicking
    // Remove.
    if edge_box.status == "retired" {
        tracing::warn!(
            edge_box_id = %edge_box.id,
            "edge-auth: rejected — box is retired"
        );
        return Err(StatusCode::UNAUTHORIZED);
    }

    let site = sites::Entity::find_by_id(edge_box.site_id)
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Single per-call edge_boxes UPDATE that batches three liveness
    // signals into one write:
    //
    //   1. `status`        — flip `pending` (or anything else) to
    //                        `active` the first time we see the box.
    //                        That's the signal that closes the
    //                        "box stuck pending in the UI even
    //                        though it's clearly running" gap.
    //   2. `tailscale_ip`  — auto-track the edge's reachable IP so
    //                        the snapshot / HLS proxies can dial it
    //                        back. Sourced from (in priority order):
    //                          a. X-Edge-Tailscale-IP header
    //                          b. X-Forwarded-For last hop
    //                          c. socket peer via ConnectInfo (set by
    //                             `into_make_service_with_connect_info`)
    //                        Always stamps `last_seen_at`. Throttling
    //                        can come if it bites.
    //   3. `funnel_hostname` — same idea for the Tailscale Funnel
    //                          hostname the worker self-reports.
    //
    // Best-effort: log on failure, never block the request.
    {
        let now = chrono::Utc::now();
        let observed_ip = peer_ip_from_request(&req);
        let observed_funnel = funnel_hostname_from_request(&req);
        let needs_status_flip = edge_box.status != "active";
        let needs_ip_update = match (observed_ip.as_deref(), edge_box.tailscale_ip.as_deref()) {
            (Some(new), stored) => stored != Some(new),
            (None, _) => false,
        };
        let needs_funnel_update = match (
            observed_funnel.as_deref(),
            edge_box.funnel_hostname.as_deref(),
        ) {
            (Some(new), stored) => stored != Some(new),
            (None, _) => false,
        };

        let mut eb_am: edge_boxes::ActiveModel = edge_box.clone().into();
        if needs_status_flip {
            eb_am.status = Set("active".into());
        }
        if needs_ip_update {
            // Safe: observed_ip is Some when needs_ip_update is true.
            eb_am.tailscale_ip = Set(observed_ip.clone());
        }
        if needs_funnel_update {
            eb_am.funnel_hostname = Set(observed_funnel.clone());
        }
        eb_am.last_seen_at = Set(Some(now.into()));
        eb_am.updated_at = Set(now.into());

        if let Err(e) = eb_am.update(&db).await {
            tracing::warn!(
                error = %e,
                edge_box_id = %edge_box.id,
                "edge-auth: liveness update failed"
            );
        } else if needs_status_flip {
            tracing::info!(
                edge_box_id = %edge_box.id,
                old_status = %edge_box.status,
                "edge-auth: status flipped to active"
            );
        }
        if needs_ip_update {
            tracing::info!(
                edge_box_id = %edge_box.id,
                old = ?edge_box.tailscale_ip,
                new = %observed_ip.as_deref().unwrap_or(""),
                "edge-auth: tailscale_ip updated from request peer"
            );
        }
        if needs_funnel_update {
            tracing::info!(
                edge_box_id = %edge_box.id,
                old = ?edge_box.funnel_hostname,
                new = %observed_funnel.as_deref().unwrap_or(""),
                "edge-auth: funnel_hostname updated from request header"
            );
        }
    }

    let ctx = EdgeContext {
        edge_box_id: edge_box.id,
        site_id: site.id,
        workspace_id: site.workspace_id,
        claim_id,
    };
    req.extensions_mut().insert(ctx);

    Ok(next.run(req).await)
}

/// What `verify_jwt` returns: the resolved edge_box plus the
/// device_claims row id that authorized this request (carried into
/// `EdgeContext` so handlers can correlate per-device audit logs).
type AuthOutcome = (edge_boxes::Model, uuid::Uuid);

/// Per-device JWT verification:
///   1. Parse + cheap structural checks
///   2. Look up `device_registry` row by `iss`
///   3. Verify HMAC-SHA256 signature with the device's secret
///   4. Resolve `device_claims` (status=claimed) → `edge_box_id`
///   5. Fetch edge_box for the rest of the middleware
///
/// A revoked claim (status='revoked') causes step 4 to miss and
/// 401s out — that's the per-request revocation guarantee Phase 3
/// was built for.
async fn verify_jwt(db: &DatabaseConnection, raw: &str) -> Result<AuthOutcome, StatusCode> {
    // Peek the issuer without verifying so we can fetch the
    // device's secret. Safe: we never touch the un-verified
    // payload until after [`jwt::verify`] has crypto-checked the
    // signature in step 3.
    let device_id = peek_jwt_issuer(raw).ok_or(StatusCode::UNAUTHORIZED)?;

    let device = device_registry::Entity::find_by_id(device_id)
        .one(db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let now = chrono::Utc::now().timestamp();
    let device_secret = crate::secrets::open(&device.device_secret).map_err(|e| {
        tracing::error!(
            device_id = %device_id,
            error = %e,
            "edge-auth: failed to open sealed device_secret"
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let verified = jwt::verify(raw, &device_secret, now).map_err(|e| {
        tracing::info!(
            device_id = %device_id,
            error = %e,
            "edge-auth: jwt rejected"
        );
        StatusCode::UNAUTHORIZED
    })?;
    if verified.device_id != device_id {
        // Defense-in-depth: shouldn't be possible because [`verify`]
        // parsed iss out of the same payload our peek read, but
        // we re-check before trusting the device_id downstream.
        return Err(StatusCode::UNAUTHORIZED);
    }

    let active_claim = device_claims::Entity::find()
        .filter(device_claims::Column::DeviceId.eq(device_id))
        .filter(device_claims::Column::Status.eq("claimed"))
        .one(db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or_else(|| {
            // Bound device with valid JWT but no claim means the
            // operator revoked or never finished bootstrap. 401
            // is the right response — the device should treat it
            // as "stop sending requests."
            tracing::info!(
                device_id = %device_id,
                "edge-auth: jwt verified but no active claim"
            );
            StatusCode::UNAUTHORIZED
        })?;
    let edge_box_id = active_claim.edge_box_id.ok_or(StatusCode::UNAUTHORIZED)?;

    let edge_box = edge_boxes::Entity::find_by_id(edge_box_id)
        .one(db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    Ok((edge_box, active_claim.id))
}

/// Pull the `iss` field out of a JWT payload without verifying
/// the signature. Used only to fetch the device's secret in step
/// 2; the result is never trusted on its own.
fn peek_jwt_issuer(token: &str) -> Option<uuid::Uuid> {
    use base64::Engine;
    let mut parts = token.split('.');
    let _ = parts.next()?;
    let payload_b64 = parts.next()?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let iss = v.get("iss")?.as_str()?;
    uuid::Uuid::parse_str(iss).ok()
}

/// Extract the edge box's reachable IP from the incoming request.
///
/// Preference order:
///   1. `X-Edge-Tailscale-IP` — explicit override the edge can set if
///      it knows its own Tailscale IP (e.g. read from
///      `tailscale status --json` on boot). Trust unconditionally
///      because the request is already JWT-authenticated.
///   2. `X-Forwarded-For` last hop — set by reverse proxies / LBs.
///      We pick the LAST entry because that's the closest hop we
///      trust (the upstream proxy); earlier entries can be spoofed.
///   3. Socket peer via [`axum::extract::ConnectInfo`] — the Tailscale-
///      native case: no reverse proxy in front, edge dials Oxy
///      directly over the tailnet. Requires the server to be served
///      with `into_make_service_with_connect_info::<SocketAddr>()`;
///      when it isn't, this falls through silently to `None`.
fn peer_ip_from_request<B>(req: &Request<B>) -> Option<String> {
    if let Some(h) = req.headers().get("x-edge-tailscale-ip")
        && let Ok(s) = h.to_str()
        && let Some(ip) = parse_literal_ip(s.trim())
    {
        return Some(ip);
    }
    if let Some(h) = req.headers().get("x-forwarded-for")
        && let Ok(s) = h.to_str()
    {
        // XFF format: "client, proxy1, proxy2" — last is the closest
        // hop. Reject anything that doesn't parse as an IP literal:
        // the value flows into `service::preview::upstream_base`
        // which builds `https://{ip}:port`, so a hostname or
        // URL-meaningful character here would be SSRF.
        let last = s.split(',').next_back()?.trim();
        if let Some(ip) = parse_literal_ip(last) {
            return Some(ip);
        }
    }
    // Socket peer via `ConnectInfo<SocketAddr>`. Inserted into
    // extensions by axum when the server is started with
    // `into_make_service_with_connect_info` — see
    // `crates/app/src/server/mod.rs`. We render with `.ip()` to drop
    // the ephemeral source port; only the IP matters for dialing the
    // box back. Loopback / unspecified addresses are filtered out
    // because they're never reachable from Oxy and would actively
    // mislead the snapshot proxy.
    if let Some(ci) = req
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
    {
        let ip = ci.0.ip();
        if !ip.is_loopback() && !ip.is_unspecified() {
            return Some(ip.to_string());
        }
    }
    None
}

/// Parse a literal IPv4 or IPv6 address. Returns the canonical
/// string form (so callers get consistent input regardless of
/// whether the client sent `100.64.001.42` or surrounding
/// brackets on a v6 literal). Returns None for hostnames, empty
/// strings, or anything containing URL-meaningful characters.
fn parse_literal_ip(s: &str) -> Option<String> {
    if s.is_empty() {
        return None;
    }
    let stripped = s.strip_prefix('[').and_then(|t| t.strip_suffix(']'));
    let candidate = stripped.unwrap_or(s);
    candidate
        .parse::<std::net::IpAddr>()
        .ok()
        .map(|ip| ip.to_string())
}

/// Extract the box's Tailscale Funnel hostname from the inbound
/// request. The edge worker reads its own funnel hostname from local
/// tailscaled (`tailscale serve status --json`) at boot and stamps
/// it on every `/control/*` call via this header. The middleware
/// records the latest value on `edge_boxes.funnel_hostname` so
/// the WebRTC session endpoint can hand the right per-box URL
/// to the operator browser.
///
/// Trust model: the request is already JWT-authenticated by the
/// time this runs, so a self-reported hostname is acceptable. But
/// `service::preview::upstream_base` later builds `https://{value}`
/// from this without re-validating, so a permissive parser here
/// turns into SSRF on the snapshot / HLS / clip proxies. Defense:
/// require a strict DNS-label syntax that ends in `.ts.net` and
/// reject any URL-meaningful character (`/`, `?`, `#`, `@`, `:`,
/// `\`) so values like `evil.com/foo.ts.net` can't slip through.
fn funnel_hostname_from_request<B>(req: &Request<B>) -> Option<String> {
    let raw = req.headers().get("x-edge-funnel-hostname")?.to_str().ok()?;
    is_valid_funnel_hostname(raw.trim()).then(|| raw.trim().to_string())
}

/// Strict `.ts.net` hostname check. Public for unit-testability.
///
/// Returns true iff `v`:
///   - is non-empty and contains no URL-meaningful characters
///     (`/`, `?`, `#`, `@`, `:`, `\`, scheme separators, whitespace),
///   - ends in `.ts.net`,
///   - has at least one label before the `.ts.net` suffix,
///   - has every dot-separated label match `[a-z0-9-]+` with no
///     leading/trailing hyphen.
fn is_valid_funnel_hostname(v: &str) -> bool {
    if v.is_empty() || !v.ends_with(".ts.net") {
        return false;
    }
    // Belt-and-suspenders: any of these would already break the
    // label check below, but listing them explicitly makes the
    // SSRF guard intent obvious to readers.
    if v.contains(['/', '?', '#', '@', ':', '\\'])
        || v.contains("://")
        || v.contains(char::is_whitespace)
    {
        return false;
    }
    let labels: Vec<&str> = v.split('.').collect();
    // ".ts.net" suffix → at minimum "<one-label>.ts.net" = 3 labels.
    if labels.len() < 3 {
        return false;
    }
    labels.iter().all(|l| {
        !l.is_empty()
            && !l.starts_with('-')
            && !l.ends_with('-')
            && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

/// Extract the bearer string from an `Authorization: Bearer <token>`
/// header. Returns `None` if the header is missing, malformed, or uses
/// a different scheme. The token itself is a per-device JWT.
fn extract_bearer<B>(req: &Request<B>) -> Option<String> {
    let h = req.headers().get(AUTHORIZATION)?;
    let s = h.to_str().ok()?;
    let trimmed = s
        .strip_prefix("Bearer ")
        .or_else(|| s.strip_prefix("bearer "))?;
    let token = trimmed.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

// Unused import warning suppression: the cameras::Entity is imported by
// downstream service code; keep the use here so the auth module's
// boundary is obvious even though the middleware itself doesn't query
// it.
#[allow(dead_code)]
fn _force_cameras_in_scope() -> Option<cameras::Model> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    fn req_with_auth(auth: &str) -> Request<Body> {
        Request::builder()
            .uri("/")
            .header(AUTHORIZATION, auth)
            .body(Body::empty())
            .unwrap()
    }

    #[test]
    fn extract_bearer_happy_path() {
        let r = req_with_auth("Bearer abc123");
        assert_eq!(extract_bearer(&r).as_deref(), Some("abc123"));
    }

    #[test]
    fn extract_bearer_case_insensitive_prefix() {
        let r = req_with_auth("bearer abc123");
        assert_eq!(extract_bearer(&r).as_deref(), Some("abc123"));
    }

    #[test]
    fn extract_bearer_rejects_missing() {
        let r = Request::builder().uri("/").body(Body::empty()).unwrap();
        assert!(extract_bearer(&r).is_none());
    }

    #[test]
    fn extract_bearer_rejects_wrong_scheme() {
        let r = req_with_auth("Basic abc123");
        assert!(extract_bearer(&r).is_none());
    }

    #[test]
    fn extract_bearer_rejects_empty() {
        let r = req_with_auth("Bearer  ");
        assert!(extract_bearer(&r).is_none());
    }

    use std::net::SocketAddr;

    fn req_with_extensions(
        ci: Option<SocketAddr>,
        xff: Option<&str>,
        edge_ip: Option<&str>,
    ) -> Request<Body> {
        let mut b = Request::builder().uri("/");
        if let Some(v) = xff {
            b = b.header("x-forwarded-for", v);
        }
        if let Some(v) = edge_ip {
            b = b.header("x-edge-tailscale-ip", v);
        }
        let mut req = b.body(Body::empty()).unwrap();
        if let Some(addr) = ci {
            req.extensions_mut()
                .insert(axum::extract::ConnectInfo(addr));
        }
        req
    }

    #[test]
    fn peer_ip_prefers_x_edge_tailscale_ip() {
        let r = req_with_extensions(
            Some("10.0.0.1:1234".parse().unwrap()),
            Some("203.0.113.5"),
            Some("100.64.1.42"),
        );
        assert_eq!(peer_ip_from_request(&r).as_deref(), Some("100.64.1.42"));
    }

    #[test]
    fn peer_ip_falls_back_to_xff_last_hop() {
        let r = req_with_extensions(
            Some("10.0.0.1:1234".parse().unwrap()),
            Some("client.example, 198.51.100.7"),
            None,
        );
        assert_eq!(peer_ip_from_request(&r).as_deref(), Some("198.51.100.7"));
    }

    #[test]
    fn peer_ip_falls_back_to_connect_info_when_no_headers() {
        let r = req_with_extensions(Some("100.64.1.42:55555".parse().unwrap()), None, None);
        assert_eq!(peer_ip_from_request(&r).as_deref(), Some("100.64.1.42"));
    }

    #[test]
    fn peer_ip_skips_connect_info_for_loopback() {
        // Loopback ConnectInfo (e.g. local dev where Oxy reaches itself)
        // is useless to record — Oxy can't dial it back as a separate
        // box. Match production behavior by returning None.
        let r = req_with_extensions(Some("127.0.0.1:55555".parse().unwrap()), None, None);
        assert!(peer_ip_from_request(&r).is_none());
    }

    #[test]
    fn peer_ip_none_when_nothing_set() {
        let r = req_with_extensions(None, None, None);
        assert!(peer_ip_from_request(&r).is_none());
    }

    #[test]
    fn peer_ip_rejects_non_ip_x_edge_tailscale_ip() {
        // The value flows into `service::preview::upstream_base`
        // verbatim. A hostname-looking value here would let a JWT-
        // authenticated caller redirect the snapshot/HLS/clip proxy
        // to an arbitrary host (SSRF). Header must be a literal IP.
        for bad in [
            "evil.com",
            "evil.com/path",
            "100.64.1.42@evil.com",
            "100.64.1.42?x=1",
            "100.64.1.42#frag",
            "100.64.1.42 evil.com",
            "",
        ] {
            let r = req_with_extensions(None, None, Some(bad));
            assert!(
                peer_ip_from_request(&r).is_none(),
                "expected rejection for {bad:?}"
            );
        }
    }

    #[test]
    fn peer_ip_accepts_ipv6_literal() {
        let r = req_with_extensions(None, None, Some("[2001:db8::1]"));
        assert_eq!(peer_ip_from_request(&r).as_deref(), Some("2001:db8::1"));
    }

    #[test]
    fn peer_ip_rejects_hostname_in_xff_last_hop() {
        let r = req_with_extensions(None, Some("client.example, evil.com"), None);
        assert!(peer_ip_from_request(&r).is_none());
    }

    #[test]
    fn funnel_hostname_accepts_canonical() {
        assert!(is_valid_funnel_hostname("video-poc-edge.tail123.ts.net"));
        assert!(is_valid_funnel_hostname("abc-123.tailnet.ts.net"));
    }

    #[test]
    fn funnel_hostname_rejects_ssrf_payloads() {
        // Every one of these used to slip past the prior `ends_with`
        // check and into `format!("https://{value}")` downstream.
        let bad = [
            "evil.com/x.ts.net",
            "evil.com#.ts.net",
            "evil.com?.ts.net",
            "evil.com@x.ts.net",
            "evil.com:80.ts.net",
            "evil.com\\x.ts.net",
            "evil .com.ts.net",
            "/x.ts.net",
            ".ts.net",     // empty leading label
            "foo..ts.net", // empty middle label
            "-foo.ts.net", // leading hyphen
            "foo-.ts.net", // trailing hyphen
            "foo.ts.net.evil.com",
            "https://x.ts.net",
        ];
        for v in bad {
            assert!(!is_valid_funnel_hostname(v), "expected rejection for {v:?}");
        }
    }
}
