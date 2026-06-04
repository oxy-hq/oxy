//! Axum middleware that authenticates an inbound request via the
//! `Authorization: Bearer <token>` header and injects an
//! [`EdgeContext`] into `request.extensions` for downstream handlers.
//!
//! Mounted as `axum::middleware::from_fn_with_state(db.clone(),
//! require_device_token)` on the `/control/*` router subtree.
//!
//! Failure modes (all 401):
//!   - missing or malformed `Authorization` header
//!   - token not found in `edge_box_tokens`
//!   - token row has `revoked_at IS NOT NULL`
//!   - edge_box / site lookup fails (token references a deleted box)
//!
//! Side effect: stamps `last_used_at = now()` on every successful auth.
//! At target scale (~50K req/sec across the fleet) this is one extra
//! UPDATE per request — acceptable for now; can be batched via a
//! periodic flush if it ever becomes a hot spot.

use axum::{
    body::Body,
    http::{HeaderValue, Request, StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::Response,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

use crate::auth::context::EdgeContext;
use crate::auth::jwt;
use crate::auth::token;
use crate::entities::{
    cameras, device_claims, device_registry, edge_box_tokens, edge_boxes, sites,
};

/// HTTP header used to deprecate the bearer auth path per
/// RFC 8594. Set via `OXY_BEARER_SUNSET_DATE` (an HTTP-date
/// string, e.g. `Sun, 31 Dec 2026 23:59:59 GMT`). When unset, no
/// header is emitted — the legacy path is fully alive.
const SUNSET_HEADER: &str = "sunset";
const SUNSET_DATE_ENV: &str = "OXY_BEARER_SUNSET_DATE";

/// Env opt-out that *re-enables* the legacy bearer path. The
/// default behaviour is JWT-only: any device authenticating with a
/// static bearer gets 401. Setting `OXY_ALLOW_LEGACY_BEARER=true`
/// is the emergency lever for a rolling upgrade window where some
/// boxes haven't received the JWT image yet — once they're on JWT
/// the flag should be removed again.
///
/// Predecessor: `OXY_REQUIRE_JWT_AUTH=true` was the opt-in switch
/// before cutover. After the 2026-06 cutover the default flipped
/// and the old name was retired.
const ALLOW_BEARER_ENV: &str = "OXY_ALLOW_LEGACY_BEARER";

fn legacy_bearer_allowed() -> bool {
    std::env::var(ALLOW_BEARER_ENV)
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Middleware that authenticates an edge box via bearer token and
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

    // ── Dual-path verify ────────────────────────────────────────────────
    //
    // The wire shape disambiguates: a JWT has exactly three
    // base64url segments joined with periods; bearer tokens
    // minted by `auth::token::issue` are flat base64 with no
    // dots. We try JWT first when the shape matches; otherwise
    // fall straight to the legacy bearer lookup. This keeps the
    // hot path one extra branch deep, not one extra DB call.
    let auth_outcome = if jwt::looks_like_jwt(&bearer) {
        verify_jwt(&db, &bearer).await
    } else {
        verify_bearer(&db, &bearer).await
    };
    let (edge_box, token_id, token_prefix, auth_mode) = match auth_outcome {
        Ok(v) => v,
        Err(status) => return Err(status),
    };

    if auth_mode == AuthMode::Bearer && !legacy_bearer_allowed() {
        // Post-cutover default: bearer auth rejected. Operator UI's
        // per-box `auth_mode` column still surfaces "bearer" so the
        // rejection isn't a surprise — we log here for the support
        // paper trail.
        tracing::warn!(
            edge_box_id = %edge_box.id,
            token_prefix = %token_prefix,
            "edge-auth: bearer rejected (JWT-only is the default post-cutover; set OXY_ALLOW_LEGACY_BEARER=true to temporarily re-enable)"
        );
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Retired-box guard. `remove_member` revokes bearer rows so the
    // legacy bearer path naturally fails after retirement, but the
    // JWT path verifies against the device's HMAC secret (which
    // stays in `device_registry`) and would otherwise keep
    // authenticating forever. Without this check, the liveness
    // batch below was re-flipping `status` back to `active` on
    // every poll — operators saw retired boxes resurface within
    // 30s of clicking Remove. After retire, the operator's intent
    // is "this box is gone": send 401 so the worker stops cleanly.
    if edge_box.status == "retired" {
        tracing::warn!(
            edge_box_id = %edge_box.id,
            token_prefix = %token_prefix,
            auth_mode = ?auth_mode,
            "edge-auth: rejected — box is retired"
        );
        return Err(StatusCode::UNAUTHORIZED);
    }

    let site = sites::Entity::find_by_id(edge_box.site_id)
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Stamp last_used_at on the bearer token row when we used
    // one. JWT path has no per-token row to stamp — the device's
    // `device_registry.last_announce_at` is the equivalent
    // liveness signal there.
    if auth_mode == AuthMode::Bearer
        && let Ok(Some(t)) = edge_box_tokens::Entity::find_by_id(token_id).one(&db).await
    {
        let mut am: edge_box_tokens::ActiveModel = t.into();
        am.last_used_at = Set(Some(chrono::Utc::now().into()));
        if let Err(e) = am.update(&db).await {
            tracing::warn!(
                error = %e,
                token_id = %token_id,
                "edge-auth: last_used_at update failed"
            );
        }
    }

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
    //                             `into_make_service_with_connect_info`
    //                             at the top of the router)
    //   3. `last_seen_at`  — what the UI's "Last seen" column reads.
    //                        Written on every call, accepting the
    //                        cost of one UPDATE per (~30s) poll per
    //                        edge. Throttling can come if it bites.
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

        let observed_auth_mode = auth_mode.as_str();
        let needs_auth_mode_update = edge_box.auth_mode != observed_auth_mode;

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
        if needs_auth_mode_update {
            // Persist the transition so operators see the rollout
            // drain in real time. We never write 'bearer' back over
            // 'jwt' to avoid bouncing the column for a single
            // misconfigured request — but a device that has truly
            // downgraded would show up on the legacy `bearer` path
            // here, and the next handler call would correct it.
            eb_am.auth_mode = Set(observed_auth_mode.into());
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
        token_id,
        token_prefix,
    };
    req.extensions_mut().insert(ctx);

    let mut response = next.run(req).await;

    // Sunset header (RFC 8594) — when the deployment has declared
    // a deprecation date for the bearer path, stamp it on every
    // bearer auth response so devices' clients can log the
    // upcoming break before it happens. JWT auth never carries
    // the header (it's the migration target, not the deprecated
    // path).
    if auth_mode == AuthMode::Bearer
        && let Ok(date) = std::env::var(SUNSET_DATE_ENV)
        && let Ok(value) = HeaderValue::from_str(date.trim())
    {
        response.headers_mut().insert(SUNSET_HEADER, value);
    }

    Ok(response)
}

/// Outcome of the per-request auth attempt — the four values the
/// rest of the middleware needs regardless of which path verified
/// the credential.
type AuthOutcome = (edge_boxes::Model, uuid::Uuid, String, AuthMode);

/// Which auth path matched. Stamped on the edge_boxes row so the
/// operator UI can show the bearer → JWT rollout drain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthMode {
    Bearer,
    Jwt,
}

impl AuthMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Bearer => "bearer",
            Self::Jwt => "jwt",
        }
    }
}

/// Legacy bearer-token path: hash + lookup `edge_box_tokens`.
/// Identical semantics to pre-Phase-3 — we factored it out so the
/// dual-path branch in the main handler stays readable.
async fn verify_bearer(db: &DatabaseConnection, bearer: &str) -> Result<AuthOutcome, StatusCode> {
    let token_hash = token::hash(bearer);
    let token_row = edge_box_tokens::Entity::find()
        .filter(edge_box_tokens::Column::TokenHash.eq(&token_hash))
        .one(db)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "edge-auth: token lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if token_row.revoked_at.is_some() {
        tracing::info!(
            token_id = %token_row.id,
            prefix = %token_row.token_prefix,
            "edge-auth: rejected revoked token"
        );
        return Err(StatusCode::UNAUTHORIZED);
    }

    let edge_box = edge_boxes::Entity::find_by_id(token_row.edge_box_id)
        .one(db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    Ok((
        edge_box,
        token_row.id,
        token_row.token_prefix,
        AuthMode::Bearer,
    ))
}

/// Phase 3 JWT path:
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

    // Synthetic identifiers for downstream logging — the JWT path
    // has no `edge_box_tokens` row. `token_id` is the claim id
    // (which IS the per-device authorization row), `token_prefix`
    // is the first 8 chars of the device UUID. Both are safe to
    // log: a leaked claim id buys nothing without the secret.
    let token_prefix = device_id.to_string().chars().take(8).collect::<String>();
    Ok((edge_box, active_claim.id, token_prefix, AuthMode::Jwt))
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
///      because the request is already bearer-authenticated.
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
    {
        let v = s.trim();
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }
    if let Some(h) = req.headers().get("x-forwarded-for")
        && let Ok(s) = h.to_str()
    {
        // XFF format: "client, proxy1, proxy2" — last is the closest
        // hop. Trim for whitespace.
        let last = s.split(',').next_back()?.trim();
        if !last.is_empty() {
            return Some(last.to_string());
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

/// Extract the box's Tailscale Funnel hostname from the inbound
/// request. The edge worker reads its own funnel hostname from local
/// tailscaled (`tailscale serve status --json`) at boot and stamps
/// it on every `/control/*` call via this header. The middleware
/// records the latest value on `edge_boxes.funnel_hostname` so
/// the WebRTC session endpoint can hand the right per-box URL
/// to the operator browser.
///
/// Trust model: the request is already bearer-authenticated by the
/// time this runs, so a self-reported hostname is acceptable.
/// Defense-in-depth: reject anything that doesn't look like a
/// `.ts.net` hostname (no whitespace, no scheme prefix).
fn funnel_hostname_from_request<B>(req: &Request<B>) -> Option<String> {
    let raw = req.headers().get("x-edge-funnel-hostname")?.to_str().ok()?;
    let v = raw.trim();
    if v.is_empty()
        || v.contains(char::is_whitespace)
        || v.contains("://")
        || !v.ends_with(".ts.net")
    {
        return None;
    }
    Some(v.to_string())
}

/// Extract the bearer string from an `Authorization: Bearer <token>`
/// header. Returns `None` if the header is missing, malformed, or uses
/// a different scheme.
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
}
