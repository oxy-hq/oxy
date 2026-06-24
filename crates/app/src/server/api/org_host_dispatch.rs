//! Host-based dispatch for bare **org subdomains** — e.g.
//! `pokehouse.oxygen-hq.com`.
//!
//! Distinct from [`super::customer_apps_host_dispatch`], which handles the
//! structural `<org>--<slug>.customer-apps[-env].<zone>` customer-app hosts.
//! A bare org subdomain is structurally identical to the admin host
//! (`app.oxygen-hq.com`), so routing here is NOT a pure structural match: it
//! needs a reserved-label guard plus a (cached) DB lookup against the
//! opt-in `org_subdomains` table.
//!
//! ## What it does
//!
//! On a request whose `Host` is `<label>.<org-zone>` and `<label>` is not
//! reserved:
//!
//!   1. `/api/*` → pass through unrewritten. The product API is same-origin
//!      and the `.oxygen-hq.com` session cookie authenticates it; the data
//!      plane stays host-agnostic.
//!   2. `/a/<slug>/…` → rewrite to `/customer-apps/<org>/<slug>/…` so the
//!      org's custom apps serve through the existing
//!      [`super::customer_apps_serve`] handler with a clean, term-free URL.
//!   3. Anything else (product SPA routes) → attach an [`OrgSubdomainCtx`]
//!      request extension so the static handler injects
//!      `window.__OXY_ORG__` into `index.html`, and bounce an
//!      unauthenticated navigation to the **app host** login (centralized
//!      auth — one OAuth callback for the whole fleet, no per-org domain).
//!
//! Unknown / disabled subdomain → 302 to the app host root. In local dev
//! (no org zone derivable) the middleware is entirely inert.

use axum::body::Body;
use axum::extract::Request;
use axum::http::header::{self, HeaderMap};
use axum::http::{StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use entity::prelude::{OrgSubdomains, Organizations};
use entity::{org_subdomains, organizations};
use oxy::database::client::establish_connection;
use oxy_auth::constants::SESSION_COOKIE_NAME;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Resolved identity for an org-subdomain request. Cloned into the request
/// extensions and serialized into `window.__OXY_ORG__`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrgSubdomainCtx {
    pub org_id: Uuid,
    /// The org slug — this IS the subdomain label (`<slug>.<zone>`).
    pub org_slug: String,
    pub default_workspace_id: Option<Uuid>,
}

// ── Reserved labels ──────────────────────────────────────────────────────

/// First labels that must never resolve to an org subdomain — infra hosts,
/// the product/admin host, and the customer-apps zone. Anything here on the
/// org zone passes straight through to its normal handler.
const RESERVED_SUBDOMAINS: &[&str] = &[
    "app", "aip", "www", "api", "auth", "admin", "docs", "static", "assets", "cdn", "mail",
    "status", "login", "signup", "help", "support", "blog", "mx", "ns",
];

/// True when `label` is reserved and must not be treated as an org
/// subdomain. Covers the constant list, the `app-<env>` admin hosts, the
/// `customer-apps[-env]` zone, and an `OXY_RESERVED_SUBDOMAINS` (comma-sep)
/// operator override.
pub fn is_reserved_label(label: &str) -> bool {
    let l = label.trim().to_ascii_lowercase();
    if l.is_empty() {
        return true;
    }
    if RESERVED_SUBDOMAINS.contains(&l.as_str()) {
        return true;
    }
    // Admin hosts (`app-dev`, `app-staging`, …) and the customer-apps zone.
    if l.starts_with("app-") || l == "customer-apps" || l.starts_with("customer-apps-") {
        return true;
    }
    if let Ok(extra) = std::env::var("OXY_RESERVED_SUBDOMAINS") {
        if extra
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .any(|s| !s.is_empty() && s == l)
        {
            return true;
        }
    }
    false
}

// ── Org-zone derivation ──────────────────────────────────────────────────

/// The DNS zone under which bare org subdomains live for this deployment.
///
/// - `OXY_ORG_SUBDOMAIN_ZONE` (leading dot tolerated) wins — required for
///   non-prod, where the apex wildcard can't be shared across envs (set it
///   to e.g. `dev.oxygen-hq.com` so `pokehouse.dev.oxygen-hq.com` resolves).
/// - Otherwise derive from `OXY_API_URL`: the admin host's parent zone, but
///   **only** when the admin host's first label is exactly `app` (prod
///   apex, `app.oxygen-hq.com` → `oxygen-hq.com`). An `app-<env>` or custom
///   host returns `None` to avoid colliding the apex wildcard across envs —
///   those must set `OXY_ORG_SUBDOMAIN_ZONE` explicitly.
pub fn org_subdomain_zone() -> Option<String> {
    if let Ok(z) = std::env::var("OXY_ORG_SUBDOMAIN_ZONE") {
        let z = z.trim().trim_start_matches('.').trim_end_matches('.');
        if !z.is_empty() {
            return Some(z.to_ascii_lowercase());
        }
    }
    let api_url = std::env::var("OXY_API_URL").ok()?;
    let parsed: url::Url = api_url.parse().ok()?;
    let admin_host = parsed.host_str()?;
    let (first_label, rest) = admin_host.split_once('.')?;
    if first_label == "app" {
        Some(rest.to_ascii_lowercase())
    } else {
        None
    }
}

/// Parse a `Host` header into a candidate org-subdomain label, using the
/// deployment's org zone. Returns `None` when the feature is inert (no zone)
/// or the host doesn't shape `<label>.<zone>` with a non-reserved,
/// single-label, dot-free prefix.
pub fn parse_org_subdomain(host: &str) -> Option<String> {
    let zone = org_subdomain_zone()?;
    parse_org_subdomain_in_zone(host, &zone)
}

/// Pure core of [`parse_org_subdomain`] — testable without env.
pub fn parse_org_subdomain_in_zone(host: &str, zone: &str) -> Option<String> {
    let host_no_port = host.split(':').next().unwrap_or(host);
    let host_no_port = host_no_port.trim_end_matches('.'); // tolerate FQDN trailing dot
    let suffix = format!(".{zone}");
    let label = host_no_port.strip_suffix(&suffix)?;
    // A multi-label prefix (`a.b.<zone>`) or empty prefix must NOT match —
    // mirrors the wildcard-hijack defense in customer_apps_host_dispatch.
    if label.is_empty() || label.contains('.') {
        return None;
    }
    if is_reserved_label(label) {
        return None;
    }
    Some(label.to_ascii_lowercase())
}

// ── TTL cache (label → resolved org, incl. negative entries) ─────────────

struct CacheEntry {
    at: Instant,
    value: Option<OrgSubdomainCtx>,
}

fn cache() -> &'static Mutex<HashMap<String, CacheEntry>> {
    static C: OnceLock<Mutex<HashMap<String, CacheEntry>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

const CACHE_TTL: Duration = Duration::from_secs(60);
/// Crude bound: bare-apex probes can spray distinct labels at us. When the
/// map crosses this, clear it wholesale (cheap, correctness-preserving).
const CACHE_MAX: usize = 4096;

/// `Some(inner)` is a hit (inner `None` = negatively cached "no such
/// subdomain"); outer `None` is a miss / expired.
fn cache_get(label: &str) -> Option<Option<OrgSubdomainCtx>> {
    let map = cache().lock().ok()?;
    let entry = map.get(label)?;
    if entry.at.elapsed() > CACHE_TTL {
        return None;
    }
    Some(entry.value.clone())
}

fn cache_put(label: &str, value: Option<OrgSubdomainCtx>) {
    if let Ok(mut map) = cache().lock() {
        if map.len() >= CACHE_MAX {
            map.clear();
        }
        map.insert(
            label.to_string(),
            CacheEntry {
                at: Instant::now(),
                value,
            },
        );
    }
}

/// Drop **this process's** cached resolutions. Called by the admin
/// enable/disable endpoint on any write. The endpoint is FleetOk, so this
/// only clears the writing replica's map — other replicas keep stale entries
/// until their own TTL lapses. Convergence is therefore **TTL-bounded
/// (≤ `CACHE_TTL`, 60s)**, not instant; immediate fleet-wide effect would need
/// a cross-process invalidation signal (follow-up).
pub fn invalidate_cache() {
    if let Ok(mut map) = cache().lock() {
        map.clear();
    }
}

async fn resolve(label: &str) -> Option<OrgSubdomainCtx> {
    if let Some(hit) = cache_get(label) {
        return hit;
    }
    match resolve_from_db(label).await {
        // Cache only a *definitive* outcome (found, or no-such-enabled-subdomain).
        Ok(resolved) => {
            cache_put(label, resolved.clone());
            resolved
        }
        // A transient lookup failure (DB connect / query error) is NOT cached —
        // otherwise a sub-second blip would negative-cache a healthy org and
        // 302 its live subdomain to the app host for the full TTL. This single
        // request falls through as "unknown"; the next one retries.
        Err(()) => None,
    }
}

/// `Ok(Some)` = an enabled org subdomain; `Ok(None)` = definitively no such
/// enabled subdomain for this slug (safe to negative-cache); `Err(())` = a
/// transient lookup failure — the caller must NOT cache it.
///
/// The lookup key is the org slug, so the subdomain follows the slug: renaming
/// an org slug repoints (and breaks the old) `<slug>.<zone>` URL. The slug now
/// carries an external-URL contract — guard slug edits accordingly (follow-up).
async fn resolve_from_db(label: &str) -> Result<Option<OrgSubdomainCtx>, ()> {
    let db = establish_connection().await.map_err(|e| {
        tracing::warn!("org subdomain resolve: db connect failed: {e}");
    })?;
    // The subdomain label is the org slug. Resolve the org first, then check
    // it has an enabled subdomain row.
    let org = match Organizations::find()
        .filter(organizations::Column::Slug.eq(label))
        .one(&db)
        .await
    {
        Ok(Some(org)) => org,
        Ok(None) => return Ok(None),
        Err(e) => {
            tracing::warn!("org subdomain resolve: org lookup failed: {e}");
            return Err(());
        }
    };
    let row = match OrgSubdomains::find()
        .filter(org_subdomains::Column::OrgId.eq(org.id))
        .filter(org_subdomains::Column::Enabled.eq(true))
        .one(&db)
        .await
    {
        Ok(Some(row)) => row,
        Ok(None) => return Ok(None),
        Err(e) => {
            tracing::warn!("org subdomain resolve: subdomain lookup failed: {e}");
            return Err(());
        }
    };
    Ok(Some(OrgSubdomainCtx {
        org_id: org.id,
        org_slug: org.slug,
        default_workspace_id: row.default_workspace_id,
    }))
}

// ── Middleware ────────────────────────────────────────────────────────────

/// Tower middleware mounted on the outer router, AFTER
/// [`super::customer_apps_host_dispatch::subdomain_rewrite_middleware`]. See
/// the module docs for the per-request decision tree.
pub async fn org_host_dispatch_middleware(request: Request, next: Next) -> Response {
    let Some(host) = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
    else {
        return next.run(request).await;
    };
    let Some(label) = parse_org_subdomain(host) else {
        return next.run(request).await;
    };

    let path = request.uri().path().to_string();

    // (1) Same-origin product API — never rewrite; cookie authenticates it.
    if path == "/api" || path.starts_with("/api/") {
        return next.run(request).await;
    }

    let Some(ctx) = resolve(&label).await else {
        // Unknown / disabled subdomain. Bounce to the app host so a probe
        // can't distinguish a real org subdomain from a fake one, and so a
        // mistyped label lands somewhere useful.
        return redirect_unknown_to_app();
    };

    // (2) Custom app under the org subdomain: /a/<slug>/… → canonical path.
    if let Some(new_path) = rewrite_app_path(&path, &ctx.org_slug) {
        let Some(new_uri) = rewrite_uri_path(request.uri(), &new_path) else {
            return next.run(request).await;
        };
        let (mut parts, body) = request.into_parts();
        parts.uri = new_uri;
        return next.run(Request::from_parts(parts, body)).await;
    }

    // (3) Product SPA route. Centralize auth: an unauthenticated navigation
    // goes to the app-host login (which owns OAuth), then `return_to` bounces
    // back here on the `.oxygen-hq.com` cookie.
    if is_html_navigation(request.method(), request.headers())
        && !has_session_cookie(request.headers())
    {
        return redirect_to_app_login(request.headers(), request.uri());
    }

    // Authenticated (or a non-navigation asset request): attach identity so
    // the static handler injects `window.__OXY_ORG__` into index.html.
    let mut request = request;
    request.extensions_mut().insert(ctx);
    next.run(request).await
}

/// `/a/<slug>/<rest>` → `/customer-apps/<org_slug>/<slug>/<rest>`. Returns
/// `None` when the path isn't an `/a/` app path (so it falls through to the
/// product SPA).
fn rewrite_app_path(path: &str, org_slug: &str) -> Option<String> {
    let rest = path.strip_prefix("/a/")?;
    if rest.is_empty() {
        return None;
    }
    let slug_end = rest.find('/').unwrap_or(rest.len());
    let slug = &rest[..slug_end];
    if slug.is_empty() {
        return None;
    }
    let after = &rest[slug_end..]; // leading '/' or empty
    Some(format!("/customer-apps/{org_slug}/{slug}{after}"))
}

/// Replace only the path of `original`, preserving scheme, authority, query.
fn rewrite_uri_path(original: &Uri, new_path: &str) -> Option<Uri> {
    let path_and_query = match original.query() {
        Some(q) => format!("{new_path}?{q}"),
        None => new_path.to_string(),
    };
    let mut builder = Uri::builder().path_and_query(path_and_query);
    if let Some(scheme) = original.scheme() {
        builder = builder.scheme(scheme.clone());
    }
    if let Some(authority) = original.authority() {
        builder = builder.authority(authority.clone());
    }
    builder.build().ok()
}

fn is_html_navigation(method: &axum::http::Method, headers: &HeaderMap) -> bool {
    if *method != axum::http::Method::GET {
        return false;
    }
    headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|a| a.contains("text/html") || a.contains("application/xhtml+xml"))
        .unwrap_or(false)
}

fn has_session_cookie(headers: &HeaderMap) -> bool {
    let needle = format!("{SESSION_COOKIE_NAME}=");
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(|c| c.split(';').any(|kv| kv.trim().starts_with(&needle)))
        .unwrap_or(false)
}

fn request_base_url(headers: &HeaderMap) -> String {
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_ascii_lowercase())
        .unwrap_or_else(|| "https".to_string());
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(header::HOST))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim())
        .unwrap_or("localhost");
    format!("{scheme}://{host}")
}

fn redirect_to_app_login(headers: &HeaderMap, uri: &Uri) -> Response {
    let return_to = format!("{}{}", request_base_url(headers), uri);
    let login_base = super::customer_apps_host_dispatch::admin_base_url()
        .unwrap_or_else(|| request_base_url(headers));
    let target = format!(
        "{login_base}/login?return_to={}",
        urlencoding::encode(&return_to)
    );
    Redirect::to(&target).into_response()
}

fn redirect_unknown_to_app() -> Response {
    match super::customer_apps_host_dispatch::admin_base_url() {
        Some(base) => Redirect::to(&format!("{base}/")).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

// ── index.html injection ──────────────────────────────────────────────────

#[derive(Serialize)]
struct OrgInjectedConfig<'a> {
    #[serde(rename = "orgId")]
    org_id: Uuid,
    #[serde(rename = "orgSlug")]
    org_slug: &'a str,
    #[serde(rename = "subdomain")]
    subdomain: &'a str,
    #[serde(rename = "defaultProjectId")]
    default_project_id: Option<Uuid>,
    /// Centralized auth host so the SPA's 401 handler can bounce re-auth
    /// there. `None` when the app host isn't derivable (local dev).
    #[serde(rename = "appBaseUrl")]
    app_base_url: Option<String>,
}

/// Splice `window.__OXY_ORG__` into an HTML response's `<head>` when the
/// org-subdomain context is present. Non-HTML responses pass through
/// untouched. Mirrors `customer_apps_serve::inject_app_config`.
pub async fn inject_org_into_response(resp: Response, ctx: &OrgSubdomainCtx) -> Response {
    let is_html = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|c| c.starts_with("text/html"))
        .unwrap_or(false);
    if !is_html {
        return resp;
    }
    let (mut parts, body) = resp.into_parts();
    let bytes = match axum::body::to_bytes(body, 16 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("org inject: failed to buffer html body: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR).into_response();
        }
    };
    let injected = splice_org_script(&bytes, ctx);
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, Body::from(injected))
}

fn splice_org_script(bytes: &[u8], ctx: &OrgSubdomainCtx) -> Vec<u8> {
    let cfg = OrgInjectedConfig {
        org_id: ctx.org_id,
        org_slug: &ctx.org_slug,
        subdomain: &ctx.org_slug,
        default_project_id: ctx.default_workspace_id,
        app_base_url: super::customer_apps_host_dispatch::admin_base_url(),
    };
    let json = match serde_json::to_string(&cfg) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("org inject: serialise failed: {e}");
            return bytes.to_vec();
        }
    };
    let escaped = json
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace("</", "<\\/");
    let snippet = format!("<script>window.__OXY_ORG__=JSON.parse('{escaped}');</script>");
    let needle = b"</head>";
    let Some(pos) = bytes.windows(needle.len()).position(|w| w == needle) else {
        tracing::warn!("org inject: no </head> in served HTML; skipping injection");
        return bytes.to_vec();
    };
    let mut out = Vec::with_capacity(bytes.len() + snippet.len());
    out.extend_from_slice(&bytes[..pos]);
    out.extend_from_slice(snippet.as_bytes());
    out.extend_from_slice(&bytes[pos..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    // ── reserved labels ───────────────────────────────────────────────────

    #[test]
    fn reserved_covers_infra_and_admin_hosts() {
        for l in [
            "app",
            "aip",
            "www",
            "api",
            "app-dev",
            "app-staging",
            "customer-apps",
            "customer-apps-dev",
            "",
        ] {
            assert!(is_reserved_label(l), "{l} should be reserved");
        }
    }

    #[test]
    fn reserved_allows_normal_org_labels() {
        for l in ["pokehouse", "acme", "mars", "my-org", "store123"] {
            assert!(!is_reserved_label(l), "{l} should NOT be reserved");
        }
    }

    #[test]
    fn reserved_env_override_adds_labels() {
        let _g = env_lock().lock().unwrap();
        unsafe {
            std::env::set_var("OXY_RESERVED_SUBDOMAINS", "pokehouse, special");
        }
        assert!(is_reserved_label("pokehouse"));
        assert!(is_reserved_label("special"));
        assert!(!is_reserved_label("acme"));
        unsafe {
            std::env::remove_var("OXY_RESERVED_SUBDOMAINS");
        }
    }

    // ── zone parse ────────────────────────────────────────────────────────

    #[test]
    fn parse_matches_bare_org_subdomain() {
        assert_eq!(
            parse_org_subdomain_in_zone("pokehouse.oxygen-hq.com", "oxygen-hq.com"),
            Some("pokehouse".to_string())
        );
    }

    #[test]
    fn parse_strips_port_and_trailing_dot() {
        assert_eq!(
            parse_org_subdomain_in_zone("pokehouse.oxygen-hq.com:443", "oxygen-hq.com"),
            Some("pokehouse".to_string())
        );
        assert_eq!(
            parse_org_subdomain_in_zone("pokehouse.oxygen-hq.com.", "oxygen-hq.com"),
            Some("pokehouse".to_string())
        );
    }

    #[test]
    fn parse_rejects_admin_and_reserved() {
        assert_eq!(
            parse_org_subdomain_in_zone("app.oxygen-hq.com", "oxygen-hq.com"),
            None
        );
        assert_eq!(
            parse_org_subdomain_in_zone("api.oxygen-hq.com", "oxygen-hq.com"),
            None
        );
    }

    #[test]
    fn parse_rejects_bare_zone() {
        assert_eq!(
            parse_org_subdomain_in_zone("oxygen-hq.com", "oxygen-hq.com"),
            None
        );
    }

    #[test]
    fn parse_rejects_multi_label_prefix() {
        // Wildcard-hijack defense: `evil.pokehouse.<zone>` must not match.
        assert_eq!(
            parse_org_subdomain_in_zone("evil.pokehouse.oxygen-hq.com", "oxygen-hq.com"),
            None
        );
    }

    #[test]
    fn parse_rejects_customer_apps_host() {
        // The customer-apps host has a multi-label prefix → no match here
        // (it's handled by customer_apps_host_dispatch, which runs first).
        assert_eq!(
            parse_org_subdomain_in_zone("acme--store.customer-apps.oxygen-hq.com", "oxygen-hq.com"),
            None
        );
    }

    #[test]
    fn parse_rejects_other_zone() {
        assert_eq!(
            parse_org_subdomain_in_zone("pokehouse.example.com", "oxygen-hq.com"),
            None
        );
    }

    // ── zone derivation ───────────────────────────────────────────────────

    #[test]
    fn zone_from_prod_api_url() {
        let _g = env_lock().lock().unwrap();
        unsafe {
            std::env::remove_var("OXY_ORG_SUBDOMAIN_ZONE");
            std::env::set_var("OXY_API_URL", "https://app.oxygen-hq.com/api");
        }
        assert_eq!(org_subdomain_zone(), Some("oxygen-hq.com".to_string()));
        unsafe {
            std::env::remove_var("OXY_API_URL");
        }
    }

    #[test]
    fn zone_none_for_env_host_without_override() {
        let _g = env_lock().lock().unwrap();
        unsafe {
            std::env::remove_var("OXY_ORG_SUBDOMAIN_ZONE");
            std::env::set_var("OXY_API_URL", "https://app-dev.oxygen-hq.com/api");
        }
        // app-dev must NOT silently derive the apex zone (cross-env collision).
        assert_eq!(org_subdomain_zone(), None);
        unsafe {
            std::env::remove_var("OXY_API_URL");
        }
    }

    #[test]
    fn zone_override_wins() {
        let _g = env_lock().lock().unwrap();
        unsafe {
            std::env::set_var("OXY_ORG_SUBDOMAIN_ZONE", ".dev.oxygen-hq.com");
            std::env::set_var("OXY_API_URL", "https://app-dev.oxygen-hq.com/api");
        }
        assert_eq!(org_subdomain_zone(), Some("dev.oxygen-hq.com".to_string()));
        unsafe {
            std::env::remove_var("OXY_ORG_SUBDOMAIN_ZONE");
            std::env::remove_var("OXY_API_URL");
        }
    }

    #[test]
    fn zone_none_for_localhost() {
        let _g = env_lock().lock().unwrap();
        unsafe {
            std::env::remove_var("OXY_ORG_SUBDOMAIN_ZONE");
            std::env::set_var("OXY_API_URL", "http://localhost:3000/api");
        }
        assert_eq!(org_subdomain_zone(), None);
        unsafe {
            std::env::remove_var("OXY_API_URL");
        }
    }

    // ── app-path rewrite ──────────────────────────────────────────────────

    #[test]
    fn rewrite_app_path_with_tail() {
        assert_eq!(
            rewrite_app_path("/a/store-pulse/assets/x.js", "pokehouse"),
            Some("/customer-apps/pokehouse/store-pulse/assets/x.js".to_string())
        );
    }

    #[test]
    fn rewrite_app_path_root_with_slash() {
        assert_eq!(
            rewrite_app_path("/a/store-pulse/", "pokehouse"),
            Some("/customer-apps/pokehouse/store-pulse/".to_string())
        );
    }

    #[test]
    fn rewrite_app_path_no_trailing() {
        assert_eq!(
            rewrite_app_path("/a/store-pulse", "pokehouse"),
            Some("/customer-apps/pokehouse/store-pulse".to_string())
        );
    }

    #[test]
    fn rewrite_app_path_rejects_non_app_and_empty() {
        assert_eq!(rewrite_app_path("/threads/123", "pokehouse"), None);
        assert_eq!(rewrite_app_path("/a/", "pokehouse"), None);
        assert_eq!(rewrite_app_path("/", "pokehouse"), None);
    }

    // ── html navigation + cookie sniff ────────────────────────────────────

    #[test]
    fn html_navigation_detection() {
        let mut h = HeaderMap::new();
        h.insert(
            header::ACCEPT,
            "text/html,application/xhtml+xml".parse().unwrap(),
        );
        assert!(is_html_navigation(&axum::http::Method::GET, &h));
        assert!(!is_html_navigation(&axum::http::Method::POST, &h));

        let mut json = HeaderMap::new();
        json.insert(header::ACCEPT, "application/json".parse().unwrap());
        assert!(!is_html_navigation(&axum::http::Method::GET, &json));
    }

    #[test]
    fn session_cookie_sniff() {
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            format!("foo=1; {SESSION_COOKIE_NAME}=abc.def; bar=2")
                .parse()
                .unwrap(),
        );
        assert!(has_session_cookie(&h));

        let mut none = HeaderMap::new();
        none.insert(header::COOKIE, "foo=1; bar=2".parse().unwrap());
        assert!(!has_session_cookie(&none));

        assert!(!has_session_cookie(&HeaderMap::new()));
    }

    // ── injection ─────────────────────────────────────────────────────────

    #[test]
    fn splice_injects_before_head_close() {
        let ctx = OrgSubdomainCtx {
            org_id: Uuid::nil(),
            org_slug: "pokehouse".to_string(),
            default_workspace_id: Some(Uuid::nil()),
        };
        let html = b"<html><head><title>x</title></head><body></body></html>";
        let out = splice_org_script(html, &ctx);
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("window.__OXY_ORG__=JSON.parse("));
        assert!(s.contains("\"orgSlug\":\"pokehouse\""));
        // Injected before </head>.
        let script_pos = s.find("window.__OXY_ORG__").unwrap();
        let head_close = s.find("</head>").unwrap();
        assert!(script_pos < head_close);
    }

    #[test]
    fn splice_noop_without_head() {
        let ctx = OrgSubdomainCtx {
            org_id: Uuid::nil(),
            org_slug: "p".to_string(),
            default_workspace_id: None,
        };
        let html = b"<html><body>no head</body></html>";
        let out = splice_org_script(html, &ctx);
        assert_eq!(out, html.to_vec());
    }
}
