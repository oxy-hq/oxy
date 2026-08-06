//! Response headers, caching policy, content-type classification, and the
//! login / legacy-uuid redirects for custom-app serving.

use std::path::Path as StdPath;

use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Redirect, Response};
use entity::prelude::Apps;
use oxy::database::client::establish_connection;
use oxy_auth::authenticator::Authenticator;
use oxy_auth::built_in::BuiltInAuthenticator;
use oxy_auth::user::UserService;
use sea_orm::EntityTrait;
use uuid::Uuid;

/// True for "user opened a page" requests: root, any trailing-slash
/// directory, or a `.html` file. Excludes asset URLs (`/_next/...`,
/// `/static/...`, `*.js`/`*.css`/`*.svg`/etc.) and API fetches so the
/// Activity tab counts page loads, not request volume.
pub(super) fn is_html_navigation(rest: &str) -> bool {
    let path = rest.trim_start_matches('/');
    if path.is_empty() || path.ends_with('/') {
        return true;
    }
    // Strip a query string before checking the extension.
    let bare = path.split('?').next().unwrap_or(path);
    bare.ends_with(".html")
}

/// Cheap UA classifier — keeps the recorded value small + bounded so
/// the Activity tab can group rows by class without storing the full
/// UA string. Real browser navigations have UA strings starting with
/// `Mozilla/` (per RFC convention); SDK/curl-style calls don't.
pub(super) fn classify_user_agent(header: Option<&axum::http::HeaderValue>) -> String {
    let Some(v) = header.and_then(|h| h.to_str().ok()) else {
        return "unknown".to_string();
    };
    if v.starts_with("Mozilla/") {
        "browser".to_string()
    } else if v.is_empty() {
        "unknown".to_string()
    } else {
        "sdk".to_string()
    }
}

pub(super) fn redirect_to_login(headers: &HeaderMap, uri: &Uri) -> Redirect {
    // `return_to` is where the user lands after a successful login —
    // it MUST be the URL they originally hit (subdomain or subpath),
    // so the post-login bounce drops them back at the custom-app
    // entry point. `base_url(headers)` echoes the request Host, which
    // is exactly what we want for `return_to`.
    let request_base = base_url(headers);
    let return_to = format!("{request_base}{uri}");

    // The login SPA itself only lives on the admin host. If the
    // request came in on a customer-apps subdomain (no SPA there),
    // bouncing to the same subdomain's `/login` causes the dispatcher
    // to rewrite it to `/customer-apps/<org>/<slug>/login`, which is
    // another `serve_dispatch` invocation that re-runs auth, redirects
    // again, accumulating `?return_to=` each round. nginx caps that at
    // 414 Request-URI Too Large after a handful of iterations and the
    // browser surfaces "too many redirects." Use the admin host
    // explicitly when we can derive it (from `OXY_API_URL`); fall back
    // to `request_base` when we can't (local dev with localhost).
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let login_base =
        if crate::server::api::custom_apps_host_dispatch::parse_subdomain(host).is_some() {
            crate::server::api::custom_apps_host_dispatch::admin_base_url().unwrap_or(request_base)
        } else {
            request_base
        };
    let target = format!(
        "{login_base}/login?return_to={}",
        urlencoding::encode(&return_to)
    );
    Redirect::to(&target)
}

/// 301 from a legacy `/customer-apps/<uuid>/<rest>` URL to the canonical
/// `<base>/customer-apps/<org_slug>/<app_slug>/<rest>`. Looks up the row
/// before redirecting — fakes get a clean 404 so we don't leak the uuid
/// namespace to unauth'd probes via 301 vs 404 distinction.
pub(super) async fn redirect_legacy_uuid(
    uuid: Uuid,
    rest: &str,
    headers: &HeaderMap,
    uri: &Uri,
) -> Response {
    // Auth gate first — same reason as the pretty path. Probing with a uuid
    // shouldn't reveal whether it's registered.
    let identity = match BuiltInAuthenticator::new().authenticate(headers).await {
        Ok(i) => i,
        Err(_) => return redirect_to_login(headers, uri).into_response(),
    };
    if UserService::find_user_by_identity(&identity)
        .await
        .unwrap_or(None)
        .is_none()
    {
        return redirect_to_login(headers, uri).into_response();
    }

    let db = match establish_connection().await {
        Ok(db) => db,
        Err(e) => {
            tracing::error!("DB connection failed during uuid redirect for {uuid}: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let app = match Apps::find_by_id(uuid).one(&db).await {
        Ok(Some(a)) => a,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("Failed to look up custom app {uuid}: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let org = match entity::prelude::Organizations::find_by_id(app.org_id)
        .one(&db)
        .await
    {
        Ok(Some(o)) => o,
        _ => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let base = base_url(headers);
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    let target = if rest.is_empty() {
        format!("{base}/customer-apps/{}/{}/{query}", org.slug, app.slug)
    } else {
        format!(
            "{base}/customer-apps/{}/{}/{rest}{query}",
            org.slug, app.slug
        )
    };
    (StatusCode::MOVED_PERMANENTLY, [(header::LOCATION, target)]).into_response()
}

/// Returns true if the client wants HTML — used to scope the SPA fallback
/// (`bundle_dir/index.html`) to top-level navigations only. Browsers send
/// `Accept: text/html,application/xhtml+xml,...` for navigations; `fetch()`
/// and asset XHRs send `Accept: */*` or a specific MIME type. We deliberately
/// do NOT treat `*/*` as HTML, because that's exactly the case the reviewer
/// wanted to 404 cleanly (asset for a missing file shouldn't get the SPA
/// shell back at 200). Missing Accept is rare in practice (manual `curl`
/// without flags); we treat it as HTML so the SPA still renders.
pub(crate) fn wants_html(headers: &HeaderMap) -> bool {
    let Some(accept) = headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()) else {
        return true;
    };
    let accept = accept.to_ascii_lowercase();
    accept.contains("text/html") || accept.contains("application/xhtml+xml")
}

/// Derive the public base URL of this oxy instance from request headers,
/// honoring `X-Forwarded-Proto` / `X-Forwarded-Host` so the redirect target
/// matches what the *browser* sees (which can differ from how oxy is
/// configured behind a reverse proxy). Falls back to `Host` and finally
/// `http://localhost:3000` (tests / direct-bind setups).
fn base_url(headers: &HeaderMap) -> String {
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_ascii_lowercase())
        .unwrap_or_else(|| "http".to_string());
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(header::HOST))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim())
        .unwrap_or("localhost:3000");
    format!("{scheme}://{host}")
}

/// Pick a `Cache-Control` policy for a bundle response.
///
/// Content-hashed asset dirs are safe to cache forever and `immutable`: the
/// URL only changes when the bytes change. Next.js emits hashed files under
/// `_next/static/`; Vite, Astro, Rsbuild, and SvelteKit emit them under
/// `assets/`. HTML and unfingerprinted root files must revalidate so a new
/// deployment is picked up immediately.
pub(super) fn cache_control_for(request_path: &str, file_path: &StdPath) -> &'static str {
    let trimmed = request_path.trim_start_matches('/');
    if trimmed.starts_with("_next/static/") || trimmed.starts_with("assets/") {
        return "public, max-age=31536000, immutable";
    }
    match file_path.extension().and_then(|e| e.to_str()) {
        Some("html" | "htm") => "no-cache",
        _ => "public, max-age=300",
    }
}

// The ETag format and `If-None-Match` comparison are shared with the admin
// SPA shell — see `server::http_cache` for why they have one home.
pub(super) use crate::server::http_cache::{if_none_match, weak_etag as etag_for};

pub(super) fn guess_content_type(path: &StdPath) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("txt") => "text/plain; charset=utf-8",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}
