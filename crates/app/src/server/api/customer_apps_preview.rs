//! Preview-channel cookie for customer apps.
//!
//! Replaces the earlier `?view=draft` query-param scheme. The customer
//! URL `/customer-apps/<org>/<app>/` accepts no view modifier at all
//! now — it always serves published. To preview drafts, an app admin
//! flips a session-scoped cookie via the endpoints below; the serve
//! and data-products handlers read that cookie (gated on
//! `is_app_admin_email`) and route the bundle to the draft channel.
//!
//! Why a cookie and not a URL flag:
//!   - The customer URL has no discoverable surface for a draft mode.
//!     Even a curious customer reading network logs sees no
//!     `?view=draft` or other "press this to flip" affordance.
//!   - Channel state lives on the staff's browser session, not in the
//!     URL — staff can share the customer URL with anyone safely.
//!   - Defense in depth: even if the cookie name is guessed and set
//!     by a non-staff user, `is_app_admin_email` makes the server
//!     ignore it.
//!
//! Cookie is HttpOnly + SameSite=Lax + short TTL (1 hour) so an
//! accidentally-left-on toggle reverts on its own.

use axum::http::{HeaderMap, StatusCode, header::SET_COOKIE};

/// Cookie name carrying the preview-channel signal. Single boolean —
/// "draft" vs absent. Future channels (e.g. a specific snapshot id)
/// would extend the value, not introduce more cookies.
pub const PREVIEW_COOKIE_NAME: &str = "oxy_preview_draft";

/// One hour. Short enough that an accidentally-left-on toggle reverts
/// without intervention; long enough to span a normal admin session.
const PREVIEW_COOKIE_MAX_AGE_SECS: i64 = 60 * 60;

/// True when the request carries `oxy_preview_draft=1`. Caller must
/// still verify the user is a staff member — this is just the
/// cookie-parsing half. Defense in depth: a non-staff user who guesses
/// the cookie name still gets the published bundle because of that
/// downstream gate.
pub fn wants_draft_preview(headers: &HeaderMap) -> bool {
    let Some(cookie_header) = headers.get(axum::http::header::COOKIE) else {
        return false;
    };
    let Ok(raw) = cookie_header.to_str() else {
        return false;
    };
    for pair in raw.split(';') {
        let pair = pair.trim();
        let Some((name, value)) = pair.split_once('=') else {
            continue;
        };
        if name.trim() == PREVIEW_COOKIE_NAME && value.trim() == "1" {
            return true;
        }
    }
    false
}

/// Should the preview cookie carry `Secure`? Mirrors the
/// `oxy_session` auto-decide logic in `auth.rs::is_request_secure`,
/// minus the per-request `X-Forwarded-Proto` lookup since preview
/// endpoints aren't reachable until the admin UI has already been
/// loaded over the same scheme. Two signals:
///
/// 1. `OXY_SESSION_COOKIE_FORCE_SECURE=1` — explicit override that
///    also drives the session cookie.
/// 2. `OXY_SESSION_COOKIE_DOMAIN` is set to a non-localhost value —
///    if cookies are domain-scoped to a real prod host, browsers
///    require Secure anyway.
///
/// Local dev with neither signal set: returns false, so the cookie
/// still works on plain http://localhost.
fn preview_cookie_should_be_secure() -> bool {
    if std::env::var("OXY_SESSION_COOKIE_FORCE_SECURE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return true;
    }
    if let Ok(domain) = std::env::var("OXY_SESSION_COOKIE_DOMAIN") {
        let domain = domain.trim().to_ascii_lowercase();
        if !domain.is_empty() && !domain.contains("localhost") && !domain.contains("127.0.0.1") {
            return true;
        }
    }
    false
}

/// `Set-Cookie` value that flips the staff session into draft-preview
/// mode. Mirrors the session-cookie domain/path/Secure so the browser
/// scopes it consistently with auth.
fn build_preview_cookie() -> String {
    let mut parts = vec![
        format!("{PREVIEW_COOKIE_NAME}=1"),
        "Path=/".to_string(),
        format!("Max-Age={PREVIEW_COOKIE_MAX_AGE_SECS}"),
        "HttpOnly".to_string(),
        "SameSite=Lax".to_string(),
    ];
    if preview_cookie_should_be_secure() {
        parts.push("Secure".to_string());
    }
    if let Ok(domain) = std::env::var("OXY_SESSION_COOKIE_DOMAIN") {
        let domain = domain.trim();
        if !domain.is_empty() {
            parts.push(format!("Domain={domain}"));
        }
    }
    parts.join("; ")
}

/// `Set-Cookie` value that clears the preview cookie. Mirrors the
/// same Path + Domain + Secure as `build_preview_cookie` so the
/// browser actually overwrites the existing entry (Secure mismatches
/// produce a separate cookie rather than an overwrite).
fn clear_preview_cookie() -> String {
    let mut parts = vec![
        format!("{PREVIEW_COOKIE_NAME}="),
        "Path=/".to_string(),
        "Max-Age=0".to_string(),
        "HttpOnly".to_string(),
        "SameSite=Lax".to_string(),
    ];
    if preview_cookie_should_be_secure() {
        parts.push("Secure".to_string());
    }
    if let Ok(domain) = std::env::var("OXY_SESSION_COOKIE_DOMAIN") {
        let domain = domain.trim();
        if !domain.is_empty() {
            parts.push(format!("Domain={domain}"));
        }
    }
    parts.join("; ")
}

/// `POST /api/customer-apps/preview-draft` — set the staff session
/// into draft-preview mode. App-admin gated (the route layer
/// installs `oxy_owner_or_app_admin_guard_middleware`).
pub async fn enable_preview_draft() -> Result<axum::response::Response, StatusCode> {
    let mut headers = HeaderMap::new();
    let value = build_preview_cookie()
        .parse()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    headers.insert(SET_COOKIE, value);
    Ok((StatusCode::NO_CONTENT, headers).into_response())
}

/// `DELETE /api/customer-apps/preview-draft` — return to published-only
/// view for this staff session.
pub async fn disable_preview_draft() -> Result<axum::response::Response, StatusCode> {
    let mut headers = HeaderMap::new();
    let value = clear_preview_cookie()
        .parse()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    headers.insert(SET_COOKIE, value);
    Ok((StatusCode::NO_CONTENT, headers).into_response())
}

// IntoResponse import via the trait method `.into_response()` —
// pulled in here so the handler signatures stay concise.
use axum::response::IntoResponse;
