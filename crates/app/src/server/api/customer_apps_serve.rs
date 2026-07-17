//! Subpath bundle serving for customer apps.
//!
//! Serves customer-app frontends at `GET /customer-apps/{uuid}` and
//! `GET /customer-apps/{uuid}/{*rest}`, gating each request with cookie or
//! bearer auth and an org-membership check.
//!
//! Bundle source per app — see `customer_apps_source::AppSource`:
//! - `LocalFolder { path }` — the dev's `path` IS the bundle dir
//!   (the directory containing `index.html`), so this handler reads
//!   `<path>/<rest_path>` straight off disk. Whatever your bundler
//!   names the output folder (`out/`, `dist/`, `build/`, …), point at
//!   it directly.
//! - `S3` — bundle was synced to
//!   `$OXY_STATE_DIR/customer-apps/<uuid>/out/` by `POST /sync`. The
//!   `out/` segment is owned by the publish pipeline here, not the
//!   dev — CI uploads to `s3://<bucket>/apps/<uuid>/out/`.
//!
//! Authentication failures redirect to `{base}/login?return_to=<url>` rather
//! than 401 so unauthenticated visitors land in the magic-link flow
//! automatically.
//!
//! ## Authentication ordering
//!
//! Auth is checked **before** any DB lookup of the requested app. This
//! prevents an unauthenticated probe from learning whether a UUID is a real
//! registered app (real → 302 redirect; fake → 404 leaks the existence
//! signal). Order is: authenticate → load user → load app → check membership.
//!
//! ## Per-request DB load
//!
//! A Next.js bundle triggers 30–100 asset requests per page load, each
//! routed through this handler. Membership lookups are cached in process
//! with a 60-second TTL so the steady-state cost per asset is one user
//! lookup + one app lookup + a HashMap read, instead of three DB queries.

use std::path::{Component, Path as StdPath, PathBuf};

use axum::body::Body;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Redirect, Response};
use entity::prelude::Apps;
use oxy::database::client::establish_connection;
use oxy_auth::authenticator::Authenticator;
use oxy_auth::built_in::BuiltInAuthenticator;
use oxy_auth::user::UserService;
use sea_orm::ColumnTrait;
use sea_orm::EntityTrait;
use sea_orm::QueryFilter;
use tokio::fs;
use uuid::Uuid;

use super::customer_apps_auth::user_can_access_app;
use super::customer_apps_bundle_cache;
use super::customer_apps_cache::{
    CACHE_CHANNEL_LOCAL, cached_canonical_dir, cached_user, invalidate_cached_canonical_dir,
    set_cached_canonical_dir, set_cached_user,
};

/// Single entry point for `GET /customer-apps/{*path}`. Decides between
/// the legacy uuid form (redirects to the canonical pretty URL) and the
/// new pretty form (`<org_slug>/<app_slug>/<rest>`).
///
/// Auth lives inside [`serve_resolved`] so the legacy-uuid redirect path
/// can still bounce anonymous visitors through `/login?return_to=...`
/// before they ever learn whether a given uuid is a real app.
pub async fn serve_dispatch(Path(path): Path<String>, request: axum::extract::Request) -> Response {
    let (parts, body) = request.into_parts();
    let headers = parts.headers;
    let uri = parts.uri;
    let method = parts.method;

    // Strip the leading slash that axum hands us for a `{*path}` capture
    // when the URL is `/customer-apps/`. Split lazily into the first two
    // segments + the remainder; an empty path means /customer-apps/ which
    // has nothing to dispatch on.
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let mut path_parts = trimmed.splitn(2, '/');
    let first = path_parts.next().unwrap_or("");
    let rest_after_first = path_parts.next().unwrap_or("");

    // Legacy uuid form: `/customer-apps/<uuid>/<rest>` → 301 to canonical.
    if let Ok(uuid) = first.parse::<Uuid>() {
        return redirect_legacy_uuid(uuid, rest_after_first, &headers, &uri).await;
    }

    // Pretty form: `/customer-apps/<org_slug>/<app_slug>/<rest>`. Need to
    // split `rest_after_first` once more to get the app slug.
    let mut rest_parts = rest_after_first.splitn(2, '/');
    let app_slug = match rest_parts.next() {
        Some(s) if !s.is_empty() => s,
        _ => {
            // We have an org but no app component — 404.
            return StatusCode::NOT_FOUND.into_response();
        }
    };
    let rest = rest_parts.next().unwrap_or("").to_string();

    // Oxy Functions route invocation: `POST .../fn/<name>`. Dispatched
    // before `serve_pretty`'s static-bundle logic — see
    // internal-docs/2026-06-12-customer-apps-functions-design.md §11.10.
    if let Some(function_name) = rest.strip_prefix("fn/") {
        if function_name.is_empty() || function_name.contains('/') {
            return StatusCode::NOT_FOUND.into_response();
        }
        let body_bytes = match axum::body::to_bytes(
            body,
            super::customer_apps_proxy::REQUEST_BODY_LIMIT,
        )
        .await
        {
            Ok(b) => b,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        };
        // `?refresh` bypasses the opt-in function result cache (same convention
        // as the /query endpoint).
        let refresh = uri
            .query()
            .map(|q| {
                q.split('&')
                    .any(|kv| kv == "refresh" || kv.starts_with("refresh="))
            })
            .unwrap_or(false);
        return super::customer_apps_functions::handle_function_request(
            first,
            app_slug,
            function_name,
            method,
            headers,
            body_bytes,
            refresh,
        )
        .await;
    }

    serve_pretty(first, app_slug, rest, method, headers, uri, body).await
}

#[allow(clippy::too_many_arguments)]
async fn serve_pretty(
    org_slug: &str,
    app_slug: &str,
    rest: String,
    method: axum::http::Method,
    headers: HeaderMap,
    uri: Uri,
    body: axum::body::Body,
) -> Response {
    // 1. Authenticate FIRST so an unauthenticated probe can't distinguish a
    //    real registered (org, app) pair (302 redirect) from a fake one
    //    (404). No DB work happens before we know the caller has a valid
    //    session.
    let identity = match BuiltInAuthenticator::new().authenticate(&headers).await {
        Ok(i) => i,
        Err(e) => {
            // Surface auth failures so an operator can tell apart "no cookie"
            // (browser never logged in) vs "stale JWT" vs "wrong domain
            // scope". Without this the only symptom is a silent redirect to
            // login and there's no way to know why from outside the server.
            let cookie_present = headers
                .get(axum::http::header::COOKIE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.contains("oxy_session="))
                .unwrap_or(false);
            let auth_header_present = headers.get(axum::http::header::AUTHORIZATION).is_some();
            tracing::warn!(
                target: "customer_apps_serve",
                org_slug = %org_slug,
                app_slug = %app_slug,
                cookie_present,
                auth_header_present,
                error = %e,
                "auth failed, redirecting to login"
            );
            return redirect_to_login(&headers, &uri).into_response();
        }
    };

    // User lookup is cached by email — without this, every Next.js asset
    // request triggers a fresh `users` table query.
    let cache_key = identity.email.to_ascii_lowercase();
    let user = if let Some(u) = cached_user(&cache_key) {
        u
    } else {
        match UserService::find_user_by_identity(&identity).await {
            Ok(Some(u)) => {
                set_cached_user(cache_key, u.clone());
                u
            }
            Ok(None) => return redirect_to_login(&headers, &uri).into_response(),
            Err(e) => {
                tracing::error!("Failed to load user for customer app {org_slug}/{app_slug}: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    };

    // 2. Now that the caller is known, look up the org + app and check
    // membership. Joining by (org_slug, app_slug) is the only DB hit other
    // than the membership cache miss path; both indexed.
    let db = match establish_connection().await {
        Ok(db) => db,
        Err(e) => {
            tracing::error!("DB connection failed serving customer app {org_slug}/{app_slug}: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let org = match entity::prelude::Organizations::find()
        .filter(entity::organizations::Column::Slug.eq(org_slug))
        .one(&db)
        .await
    {
        Ok(Some(o)) => o,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("Failed to look up org {org_slug}: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let app = match Apps::find()
        .filter(entity::apps::Column::OrgId.eq(org.id))
        .filter(entity::apps::Column::Slug.eq(app_slug))
        .one(&db)
        .await
    {
        Ok(Some(a)) => a,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("Failed to look up customer app {org_slug}/{app_slug}: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let id = app.id;

    // Combined access check (org member | workspace grant | global app
    // admin). Cached per (user_id, app_id) for 60s — see
    // `customer_apps_auth::user_can_access_app`. Critical for the Next.js
    // asset storm (30-100 requests per page load).
    let allowed = match user_can_access_app(&db, user.id, &user.email, &app).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Access check failed for customer app {id}: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    if !allowed {
        return (
            StatusCode::FORBIDDEN,
            "You don't have access to this customer app.",
        )
            .into_response();
    }

    // 3. Dispatch through the source facade. The per-app source decision
    //    (v0 / local / s3) is recorded at register time; this handler only
    //    knows about three rendering modes and matches on them.
    let source = match super::customer_apps_source::AppSource::from_model(&app) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Bad source config for customer app {id}: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let runtime = AppRuntimeConfig::from_app(&app, &org.slug);

    // Detect the "user-navigation" requests (root path, trailing-slash
    // directory, or `.html`) so we record at most one view event per
    // user-visible page load — not once per asset / API fetch which
    // would 100× the volume with zero extra signal.
    //
    // Computed BEFORE the dispatch so the per-source-type response
    // build doesn't need to know about tracking. Tracking + cookie
    // injection happen in the post-dispatch wrapper at the bottom.
    let is_html_request = is_html_navigation(&rest);
    let source_label = match super::customer_apps_host_dispatch::parse_subdomain(
        headers
            .get(axum::http::header::HOST)
            .and_then(|v| v.to_str().ok())
            .unwrap_or(""),
    ) {
        Some(_) => "subdomain",
        None => "subpath",
    };

    use super::customer_apps_source::AppSource;
    let response = match source {
        AppSource::V0 { url } => {
            super::customer_apps_proxy::proxy(
                &url,
                &rest,
                method,
                &uri,
                &headers,
                body,
                super::customer_apps_proxy::ProxyIdentity {
                    app: &app,
                    org: &org,
                    user_id: user.id,
                    user_email: &user.email,
                },
            )
            .await
        }
        AppSource::LocalFolder { path } => {
            // LocalFolder has no draft/published split — one directory
            // serves everyone. Publishing for these sources is purely a
            // sidebar visibility toggle.
            serve_from_local(id, &path, &rest, &headers, &runtime).await
        }
        AppSource::S3 => {
            // The customer URL accepts no view modifier. Draft mode
            // lives on a staff-only HttpOnly cookie set via
            // `POST /api/customer-apps/preview-draft`. Customer's
            // browser never carries this cookie; even if a customer
            // forged it, `is_app_admin_email` denies them draft
            // access below.
            let cookie_wants_draft = super::customer_apps_preview::wants_draft_preview(&headers);
            // Fail-closed inside the one reader: a lookup error reports no standing.
            // Admin OR owner — both operator tiers reach every customer-app surface.
            let is_staff = crate::server::authz::globals::platform_standing(&db, &user.email)
                .await
                .is_staff();
            let channel =
                resolve_channel(cookie_wants_draft && is_staff, app.published_at.is_some());
            // New publish pipeline: when the channel has a build pointer,
            // serve straight from S3 (no local state dir). Legacy `s3`
            // rows leave both pointers NULL and fall through to the
            // state-dir path until they're re-published.
            use super::customer_apps_sync::Channel;
            let build_pk = match channel {
                Channel::Draft => app.draft_build_id,
                Channel::Published => app.published_build_id,
            };
            match build_pk {
                Some(build_pk) => {
                    serve_from_s3_build(&db, id, build_pk, &rest, &runtime, &headers).await
                }
                None => {
                    // Post-retirement: the legacy state-dir serve is gone.
                    // An s3-source app with no build pointer hasn't been
                    // published through the new pipeline yet (`oxy publish`).
                    tracing::warn!(
                        "app {id}: no build for {channel:?} channel — not yet published via `oxy publish`"
                    );
                    StatusCode::NOT_FOUND.into_response()
                }
            }
        }
    };

    // Post-dispatch: for browser-navigation requests only (root /
    // trailing-slash / `.html`), stamp the session cookie on the
    // response and spawn the view-event recording. Asset / API
    // fetches are storm-volume — exclude them so the Activity tab
    // counts user-visible page loads, not request volume.
    if is_html_request {
        let host = headers
            .get(axum::http::header::HOST)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("");
        let secure = uri.scheme_str() == Some("https")
            || headers
                .get("x-forwarded-proto")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.eq_ignore_ascii_case("https"))
                .unwrap_or(false);
        let (session_id, set_cookie) =
            super::customer_apps_tracking::session_id_for_serve(&headers, secure);
        let referrer = headers
            .get(axum::http::header::REFERER)
            .and_then(|v| v.to_str().ok());
        let sanitized_referrer = super::customer_apps_tracking::sanitize_referrer(referrer, host);
        let user_agent_class = classify_user_agent(headers.get(axum::http::header::USER_AGENT));
        let app_id = id;
        let user_id = user.id;
        let user_email = user.email.clone();
        let source_label = source_label.to_string();
        // Fire-and-forget; a slow DB insert must not stall the HTML
        // response. Losing a row on crash is the documented acceptable
        // failure mode for tracking-grade data.
        tokio::spawn(async move {
            super::customer_apps_tracking::record_view(
                app_id,
                user_id,
                user_email,
                session_id,
                sanitized_referrer,
                user_agent_class,
                source_label,
            )
            .await;
        });

        let mut resp = response;
        if let Ok(hv) = axum::http::HeaderValue::from_str(&set_cookie) {
            resp.headers_mut().append(header::SET_COOKIE, hv);
        }
        return resp;
    }

    response
}

/// True for "user opened a page" requests: root, any trailing-slash
/// directory, or a `.html` file. Excludes asset URLs (`/_next/...`,
/// `/static/...`, `*.js`/`*.css`/`*.svg`/etc.) and API fetches so the
/// Activity tab counts page loads, not request volume.
fn is_html_navigation(rest: &str) -> bool {
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
fn classify_user_agent(header: Option<&axum::http::HeaderValue>) -> String {
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

/// Serve a customer-app file directly from S3 (the new publish pipeline).
/// Resolves the build's files from its `app_builds` row and reads objects
/// through the in-memory bundle cache — no local state dir, so any node can
/// serve any build. HTML gets the same base-path rewrite + `window.__OXY_APP__`
/// injection as the legacy disk path; the cache key includes the build id, so
/// a promote/rollback serves fresh bytes with no explicit invalidation.
async fn serve_from_s3_build(
    db: &sea_orm::DatabaseConnection,
    app_id: Uuid,
    build_pk: Uuid,
    rest: &str,
    runtime: &AppRuntimeConfig,
    headers: &HeaderMap,
) -> Response {
    let build = match entity::app_builds::Entity::find_by_id(build_pk)
        .one(db)
        .await
    {
        Ok(Some(b)) => b,
        Ok(None) => {
            tracing::error!("app {app_id}: build pointer {build_pk} has no app_builds row");
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(e) => {
            tracing::error!("app {app_id}: failed to load build {build_pk}: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Empty or directory-style paths serve the SPA entry.
    let requested = {
        let t = rest.trim_start_matches('/');
        if t.is_empty() || t.ends_with('/') {
            format!("{t}index.html")
        } else {
            t.to_string()
        }
    };

    // Try the requested object; on a miss, fall back to the SPA shell
    // (`index.html`) — same behavior as the disk path's spa fallback.
    let (rel_used, bytes) =
        match customer_apps_bundle_cache::get_or_fetch(app_id, &build.build_id, &requested).await {
            Ok(Some(b)) => (requested.clone(), b),
            Ok(None) => match customer_apps_bundle_cache::get_or_fetch(
                app_id,
                &build.build_id,
                "index.html",
            )
            .await
            {
                Ok(Some(b)) => ("index.html".to_string(), b),
                Ok(None) => return StatusCode::NOT_FOUND.into_response(),
                Err(e) => {
                    tracing::error!("app {app_id}: S3 read index.html failed: {e}");
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            },
            Err(e) => {
                tracing::error!("app {app_id}: S3 read {requested} failed: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

    let resolved_path = StdPath::new(&rel_used);
    let mime = guess_content_type(resolved_path);
    let cache = cache_control_for(&requested, resolved_path);
    let body_bytes = if mime.starts_with("text/html") {
        let expected_prefix = format!("/customer-apps/{}/{}/", runtime.org_slug, runtime.slug);
        let rewritten =
            rewrite_bundle_base_path(bytes.as_slice(), &expected_prefix, runtime.app_id);
        inject_app_config(&rewritten, runtime, resolved_path)
    } else {
        bytes.to_vec()
    };
    if mime.starts_with("text/html") {
        let etag = etag_for(&body_bytes);
        if if_none_match(headers, &etag) {
            return (
                StatusCode::NOT_MODIFIED,
                [
                    (header::ETAG, etag),
                    (header::CACHE_CONTROL, cache.to_string()),
                ],
            )
                .into_response();
        }
        return (
            [
                (header::CONTENT_TYPE, mime.to_string()),
                (header::CACHE_CONTROL, cache.to_string()),
                (header::ETAG, etag),
            ],
            Body::from(body_bytes),
        )
            .into_response();
    }
    (
        [(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, cache)],
        Body::from(body_bytes),
    )
        .into_response()
}

/// Channel resolution for S3-source serve:
/// - staff with the preview-draft cookie set → draft
/// - app has been published → published (default for both staff and customer)
/// - otherwise → draft (only staff reaches here; the auth gate blocked
///   customers on unpublished apps)
fn resolve_channel(
    staff_wants_draft: bool,
    is_published: bool,
) -> super::customer_apps_sync::Channel {
    use super::customer_apps_sync::Channel;
    if staff_wants_draft {
        return Channel::Draft;
    }
    if is_published {
        Channel::Published
    } else {
        Channel::Draft
    }
}

/// Browser-side runtime identity for a served customer app. Injected into
/// every HTML response under `<script>window.__OXY_APP__ = {…}</script>`
/// so the bundle's SDK can call `/api/<projectId>/…` without having those
/// ids baked in at build time. See sdk/typescript/src/config.ts.
#[derive(serde::Serialize, Debug, Clone)]
pub(crate) struct AppRuntimeConfig {
    #[serde(rename = "appId")]
    pub app_id: Uuid,
    pub slug: String,
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    #[serde(rename = "orgSlug")]
    pub org_slug: String,
    #[serde(rename = "projectId")]
    pub project_id: Uuid,
    pub branch: String,
    /// Empty string = same-origin. v1 always emits empty; reserved for a
    /// future where customer apps live on a different host than the API.
    #[serde(rename = "apiBaseUrl")]
    pub api_base_url: String,
}

impl AppRuntimeConfig {
    fn from_app(app: &entity::apps::Model, org_slug: &str) -> Self {
        Self {
            app_id: app.id,
            slug: app.slug.clone(),
            org_id: app.org_id,
            org_slug: org_slug.to_string(),
            project_id: app.project_id,
            branch: app.branch.clone(),
            api_base_url: String::new(),
        }
    }
}

async fn serve_from_local(
    id: Uuid,
    configured_path: &StdPath,
    rest: &str,
    headers: &HeaderMap,
    runtime: &AppRuntimeConfig,
) -> Response {
    // The configured path IS the bundle dir — the dev points us
    // directly at the directory that holds `index.html` + assets.
    // We used to append `out/` on the assumption it was always a
    // Next.js static export; that broke Vite (`dist/`), Astro
    // (`dist/`), Rsbuild (`dist/`), and anything with a custom out
    // dir. Today the convention is explicit on the dev's side: point
    // at the right folder, however your bundler names it.
    serve_from_dir(
        id,
        CACHE_CHANNEL_LOCAL,
        configured_path,
        rest,
        headers,
        runtime,
    )
    .await
}

/// Shared serving path for both local-folder and s3 sources. Resolves the
/// per-uuid canonical bundle dir (cached), then defers to `serve_file` for
/// the path-traversal-safe + symlink-defended file read.
///
/// `channel_key` keys the cache so draft and published bundle dirs don't
/// cross-contaminate (a staff preview-draft request must never be cached
/// and served back to a customer asking for published, and vice versa).
/// Pass `CACHE_CHANNEL_LOCAL` for local-folder sources; otherwise pass
/// the active S3 channel's `.as_str()` value.
async fn serve_from_dir(
    id: Uuid,
    channel_key: &'static str,
    bundle_dir: &StdPath,
    rest: &str,
    headers: &HeaderMap,
    runtime: &AppRuntimeConfig,
) -> Response {
    let canonical_dir = if let Some(p) = cached_canonical_dir(id, channel_key) {
        p
    } else {
        match bundle_dir.canonicalize() {
            Ok(p) if p.is_dir() => {
                set_cached_canonical_dir(id, channel_key, p.clone());
                p
            }
            _ => {
                // Echo the resolved path back so the operator can see at a
                // glance whether the wrong directory is configured — common
                // cause for LocalFolder apps after the implicit `out/`
                // suffix was dropped, and for S3 apps that haven't been
                // synced yet.
                tracing::warn!(
                    "Bundle dir missing or unresolvable for customer app {id} (expected {bundle_dir:?})"
                );
                return (
                    StatusCode::NOT_FOUND,
                    format!(
                        "Bundle not deployed for customer app {id}.\n\
                         Server looked for files at: {}\n\n\
                         Common causes:\n\
                         - LocalFolder source: source_config.path doesn't point at the directory containing index.html.\n\
                         - S3 source: bundle hasn't been synced yet, or publish ran against an empty draft prefix.",
                        bundle_dir.display()
                    ),
                )
                    .into_response();
            }
        }
    };
    let allow_spa_fallback = wants_html(headers);
    let response = serve_file(&canonical_dir, rest, allow_spa_fallback, runtime).await;
    // If the symlink-escape guard fired, the canonical dir we cached points
    // at something suspicious — invalidate now so a follow-up request after
    // the operator fixes the symlink doesn't have to wait CACHE_TTL.
    if response.status() == StatusCode::FORBIDDEN {
        invalidate_cached_canonical_dir(id, channel_key);
    }
    response
}

fn redirect_to_login(headers: &HeaderMap, uri: &Uri) -> Redirect {
    // `return_to` is where the user lands after a successful login —
    // it MUST be the URL they originally hit (subdomain or subpath),
    // so the post-login bounce drops them back at the customer-app
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
    let login_base = if super::customer_apps_host_dispatch::parse_subdomain(host).is_some() {
        super::customer_apps_host_dispatch::admin_base_url().unwrap_or(request_base)
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
async fn redirect_legacy_uuid(uuid: Uuid, rest: &str, headers: &HeaderMap, uri: &Uri) -> Response {
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
            tracing::error!("Failed to look up customer app {uuid}: {e}");
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
fn wants_html(headers: &HeaderMap) -> bool {
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

/// Serve a file from `bundle_dir`. `bundle_dir` MUST be a canonicalized,
/// absolute path — the symlink-escape check at the bottom compares against
/// it as a fixed prefix.
async fn serve_file(
    bundle_dir: &StdPath,
    rest: &str,
    allow_spa_fallback: bool,
    runtime: &AppRuntimeConfig,
) -> Response {
    let Some(candidate) = resolve_safe(bundle_dir, rest) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    // Candidate order: literal path → `.html` suffix (Next.js
    // trailingSlash=false) → index.html in the requested directory. The
    // bundle-root index.html SPA fallback is appended only when the request
    // accepts HTML — that way an asset XHR for a missing file still gets a
    // proper 404 instead of an opaque 200 with the SPA shell, which makes
    // operator debugging much harder.
    let mut candidates = vec![
        candidate.clone(),
        candidate.with_extension("html"),
        candidate.join("index.html"),
    ];
    if allow_spa_fallback {
        candidates.push(bundle_dir.join("index.html"));
    }
    let resolved = first_existing(&candidates).await;

    let Some(path) = resolved else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // Symlink-escape defense: bundles come from CI output, not a hand-curated
    // directory, so a symlink inside `<bundle_dir>` that points outside it
    // (`out/secrets -> /etc/passwd`) would otherwise be served. Canonicalize
    // and verify the real file lives inside the canonical bundle root.
    let canon = match fs::canonicalize(&path).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to canonicalize {path:?}: {e}");
            return StatusCode::NOT_FOUND.into_response();
        }
    };
    if !canon.starts_with(bundle_dir) {
        tracing::warn!(
            "Refusing to serve {canon:?}: escapes bundle root {bundle_dir:?} via symlink"
        );
        return StatusCode::FORBIDDEN.into_response();
    }

    match fs::read(&canon).await {
        Ok(bytes) => {
            let mime = guess_content_type(&canon);
            let cache = cache_control_for(rest, &canon);
            // HTML responses get two passes:
            //   1. base-path rewrite — fixes the build-time-vs-serve-time
            //      mismatch that bites when an engineer renames a slug
            //      without rebuilding (the bundle's `<script src=...>`
            //      hardcodes the absolute base it was built with).
            //   2. runtime identity injection — `window.__OXY_APP__` into
            //      `<head>` so the SDK can read identity at runtime.
            //
            // Non-HTML responses pass through unchanged. The rewrite is
            // safe to skip for them because Vite/Next-style bundles only
            // emit absolute base-path strings in the HTML entry; JS/CSS
            // chunks reference each other relatively at runtime.
            let body_bytes = if mime.starts_with("text/html") {
                let expected_prefix =
                    format!("/customer-apps/{}/{}/", runtime.org_slug, runtime.slug);
                let rewritten = rewrite_bundle_base_path(&bytes, &expected_prefix, runtime.app_id);
                inject_app_config(&rewritten, runtime, &canon)
            } else {
                bytes
            };
            (
                [(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, cache)],
                Body::from(body_bytes),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to read bundle file {canon:?}: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Fix up the bundle's `index.html` when its baked base path doesn't
/// match the URL we're serving from. Common cause: an engineer
/// rebuilt locally without `OXY_APP_BASE_PATH` set, or renamed the
/// app slug after the bundle was built. Either way the bundle's
/// hardcoded `<script src="/customer-apps/<old>/<old>/...">` won't
/// resolve to anything under the current URL prefix and the browser
/// 404s every asset.
///
/// We scan for the first absolute `/customer-apps/<org>/<slug>/`
/// path in the HTML; if it differs from the prefix this request came
/// in on, we string-replace every occurrence. Vite/Astro/Rsbuild
/// SPAs load their entry script from this baked URL and resolve all
/// further chunks relative to that script's URL, so rewriting the
/// HTML alone is enough to fix the load chain.
///
/// Logs a warning on rewrite — the operator should still set
/// `OXY_APP_BASE_PATH` correctly at build time so this is a one-time
/// rescue and not a steady-state crutch.
fn rewrite_bundle_base_path(bytes: &[u8], expected_prefix: &str, app_id: Uuid) -> Vec<u8> {
    let Ok(html) = std::str::from_utf8(bytes) else {
        // Binary content claiming to be HTML — leave it alone.
        return bytes.to_vec();
    };

    // Case 1: bundle was built with SOME `/customer-apps/<org>/<slug>/`
    // base path baked in (e.g. via `OXY_APP_BASE_PATH` or Vite's
    // `base:` option). If it doesn't match the prefix this request
    // came in on (operator rebuilt locally, renamed the slug, etc.),
    // string-replace to the correct one.
    if let Some(baked) = first_customer_apps_prefix(html) {
        if baked == expected_prefix {
            return bytes.to_vec();
        }
        tracing::warn!(
            app_id = %app_id,
            baked = %baked,
            expected = %expected_prefix,
            "Bundle base path mismatch — rewriting index.html. \
             Permanent fix: rebuild with OXY_APP_BASE_PATH={expected_prefix} \
             or rename the app slug to match the baked prefix."
        );
        return html.replace(&baked, expected_prefix).into_bytes();
    }

    // Case 2: bundle was built with default base `/` (no env var, no
    // Vite `base:` configured). The HTML has bare absolute paths like
    // `<script src="/assets/index-XXX.js">`. The browser resolves
    // those against the document origin (e.g. `:5173` in dev), not
    // under `/customer-apps/<org>/<slug>/`, and every chunk 404s.
    //
    // Walk the HTML once, prefixing the value of any `src=` / `href=`
    // attribute that starts with a single `/` (not `//`, not already
    // `/customer-apps/`). Catches Vite, Astro, Rsbuild, Next-export
    // default output.
    let rewritten = prefix_bare_absolute_paths(html, expected_prefix);
    if rewritten.len() != html.len() {
        tracing::warn!(
            app_id = %app_id,
            expected = %expected_prefix,
            "Bundle built with default base path — prefixing bare absolute \
             asset URLs at serve time. Permanent fix: install \
             `@oxy-hq/vite-plugin` (or rebuild with \
             OXY_APP_BASE_PATH={expected_prefix})."
        );
        return rewritten.into_bytes();
    }
    bytes.to_vec()
}

/// Walk `html`, rewriting `src="/…"` and `href="/…"` attribute values
/// to `src="<prefix>…"` so a bundle built with the bundler's default
/// base path (`/`) still resolves all its chunks under the
/// customer-apps subpath.
///
/// Skips values that:
///   - don't start with `/` (relative or fully-qualified URLs),
///   - start with `//` (protocol-relative — those go to a different host),
///   - already start with `/customer-apps/` (defensive — this branch
///     only runs when no such prefix was found by [`first_customer_apps_prefix`],
///     but checking again is cheap insurance).
///
/// String-based rather than HTML-parser-based because the input is
/// always a bundler's generated `index.html` — there's no malformed
/// markup, no scripts to dodge, and we only touch attribute values of
/// `src` and `href`. A full parser would be ~3KB of dep weight for
/// zero functional gain here.
///
/// **Known limitations** (acceptable trade-offs, not bugs):
///   - Matches attribute-shaped substrings even when they appear
///     inside inline `<script>` JS string literals (e.g.
///     `const el = '<img src="/foo">'`). For SPA index.html files
///     this is benign because such literals are rare and the rewrite
///     is what the JS would want anyway, but a downstream consumer
///     embedding raw HTML strings in a script could see surprising
///     URL rewrites.
///   - Only matches `src=` and `href=` attribute names. Does NOT
///     touch `srcset=` (responsive images), `poster=` (videos),
///     `data=` (object tags), or CSS `url(/…)` references inside
///     inline `<style>` blocks. Modern bundlers don't emit those in
///     index.html, but a hand-written bundle that does will need to
///     bake the prefix at build time instead.
fn prefix_bare_absolute_paths(html: &str, prefix: &str) -> String {
    let prefix_trimmed = prefix.trim_end_matches('/');
    let mut out = String::with_capacity(html.len() + 256);
    let mut rest = html;

    loop {
        // Find the next ` src=` or ` href=`, whichever comes first.
        // Leading-space requirement avoids matching inside class names,
        // partial words, or JS string literals embedded in `<script>`.
        let next = [" src=", " href="]
            .iter()
            .filter_map(|pat| rest.find(pat).map(|p| (p, pat.len())))
            .min_by_key(|&(p, _)| p);
        let Some((attr_at, name_len)) = next else {
            out.push_str(rest);
            return out;
        };

        let after_eq = attr_at + name_len;
        out.push_str(&rest[..after_eq]);
        rest = &rest[after_eq..];

        // Attribute value must be quoted (single or double). Unquoted
        // attribute values are technically valid HTML but no bundler
        // emits them; skip the rewrite in that case.
        let Some(quote_char) = rest.chars().next() else {
            return out;
        };
        if quote_char != '"' && quote_char != '\'' {
            continue;
        }
        out.push(quote_char);
        rest = &rest[1..];

        let Some(close) = rest.find(quote_char) else {
            // Unterminated quoted value — broken HTML; pass through
            // unchanged from this point on rather than risk corruption.
            out.push_str(rest);
            return out;
        };
        let value = &rest[..close];
        if value.starts_with('/')
            && !value.starts_with("//")
            && !value.starts_with("/customer-apps/")
        {
            out.push_str(prefix_trimmed);
        }
        out.push_str(value);
        out.push(quote_char);
        rest = &rest[close + 1..];
    }
}

/// Find the first `/customer-apps/<org>/<slug>/` substring in the
/// HTML. Returns the full matched prefix including the trailing
/// slash, or `None` if there isn't one or the surrounding characters
/// don't look like an attribute value.
pub(crate) fn first_customer_apps_prefix(html: &str) -> Option<String> {
    const SENTINEL: &str = "/customer-apps/";
    let idx = html.find(SENTINEL)?;
    let after = &html[idx + SENTINEL.len()..];

    // Read up to two more path segments. Both must be non-empty and
    // composed of URL-safe slug chars; the second is followed by a
    // closing slash that bounds the prefix.
    let mut segments: [Option<&str>; 2] = [None, None];
    let mut cursor = 0usize;
    for slot in segments.iter_mut() {
        let rest = &after[cursor..];
        let slash_at = rest.find('/')?;
        let segment = &rest[..slash_at];
        if segment.is_empty() || !segment.chars().all(is_slug_char) {
            return None;
        }
        *slot = Some(segment);
        cursor += slash_at + 1;
    }

    let (Some(org), Some(slug)) = (segments[0], segments[1]) else {
        return None;
    };
    Some(format!("/customer-apps/{org}/{slug}/"))
}

fn is_slug_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'
}

/// Splice `<script>window.__OXY_APP__ = JSON.parse(…)</script>` into the
/// HTML's `<head>` so the bundle's SDK reads identity from a runtime
/// global instead of build-time env vars.
///
/// We `JSON.parse` a JSON-string literal rather than emit an object
/// literal — JS engines parse JSON ~2× faster, and we only have to escape
/// for one syntactic layer (string contents) instead of two (object +
/// string).
///
/// Insertion strategy: prefer `</head>` (puts the global early, before
/// any deferred scripts run). If the bundle has no `</head>`, log a
/// warning and serve untouched — the bundle was hand-rolled or pre-Next
/// and the admin needs to add one.
fn inject_app_config(bytes: &[u8], runtime: &AppRuntimeConfig, path: &StdPath) -> Vec<u8> {
    let json = match serde_json::to_string(runtime) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("Failed to serialise runtime config for {path:?}: {e}");
            return bytes.to_vec();
        }
    };
    let escaped_for_js_string = json
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        // </script> in the JSON would otherwise end the script block.
        // The HTML spec only cares about a literal `</` after `script`,
        // but it's cheaper to escape both directions of the slash.
        .replace("</", "<\\/");
    let snippet =
        format!("<script>window.__OXY_APP__=JSON.parse('{escaped_for_js_string}');</script>");

    let needle = b"</head>";
    let Some(pos) = find_subsequence(bytes, needle) else {
        tracing::warn!(
            "No </head> in {path:?}; skipping window.__OXY_APP__ injection. \
             Bundles must include a <head>…</head> for runtime identity injection."
        );
        return bytes.to_vec();
    };
    let mut out = Vec::with_capacity(bytes.len() + snippet.len());
    out.extend_from_slice(&bytes[..pos]);
    out.extend_from_slice(snippet.as_bytes());
    out.extend_from_slice(&bytes[pos..]);
    out
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Pick a `Cache-Control` policy for a bundle response.
///
/// Content-hashed asset dirs are safe to cache forever and `immutable`: the
/// URL only changes when the bytes change. Next.js emits hashed files under
/// `_next/static/`; Vite, Astro, Rsbuild, and SvelteKit emit them under
/// `assets/`. HTML and unfingerprinted root files must revalidate so a new
/// deployment is picked up immediately.
fn cache_control_for(request_path: &str, file_path: &StdPath) -> &'static str {
    let trimmed = request_path.trim_start_matches('/');
    if trimmed.starts_with("_next/static/") || trimmed.starts_with("assets/") {
        return "public, max-age=31536000, immutable";
    }
    match file_path.extension().and_then(|e| e.to_str()) {
        Some("html" | "htm") => "no-cache",
        _ => "public, max-age=300",
    }
}

/// Weak ETag over the final response bytes (post-injection for HTML). Weak
/// (`W/`) because the bytes are produced by a transform, not a raw file.
fn etag_for(bytes: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    format!("W/\"{:016x}\"", h.finish())
}

/// True when the request's `If-None-Match` already holds `etag`.
fn if_none_match(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(',').any(|t| t.trim() == etag))
        .unwrap_or(false)
}

/// Join `bundle_dir` with `rest`, rejecting paths that try to escape via
/// `..` or absolute components. Without this guard a request for
/// `../../etc/passwd` would happily traverse out of the bundle root.
fn resolve_safe(bundle_dir: &StdPath, rest: &str) -> Option<PathBuf> {
    let rest = rest.trim_start_matches('/');
    let rel = StdPath::new(rest);
    for c in rel.components() {
        match c {
            Component::Normal(_) | Component::CurDir => {}
            _ => return None,
        }
    }
    Some(bundle_dir.join(rel))
}

async fn first_existing(paths: &[PathBuf]) -> Option<PathBuf> {
    for p in paths {
        if let Ok(meta) = fs::metadata(p).await
            && meta.is_file()
        {
            return Some(p.clone());
        }
    }
    None
}

fn guess_content_type(path: &StdPath) -> &'static str {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_runtime() -> AppRuntimeConfig {
        AppRuntimeConfig {
            app_id: Uuid::nil(),
            slug: "acme-analytics".to_string(),
            org_id: Uuid::nil(),
            org_slug: "acme".to_string(),
            project_id: Uuid::nil(),
            branch: "main".to_string(),
            api_base_url: String::new(),
        }
    }

    #[test]
    fn inject_app_config_splices_before_closing_head() {
        let html = b"<!doctype html><html><head><title>X</title></head><body/></html>";
        let out = inject_app_config(html, &fake_runtime(), std::path::Path::new("test.html"));
        let s = std::str::from_utf8(&out).unwrap();
        assert!(s.contains("window.__OXY_APP__=JSON.parse"));
        // Verify ordering: script lands inside <head>, before </head>.
        let script_pos = s.find("window.__OXY_APP__").unwrap();
        let close_head_pos = s.find("</head>").unwrap();
        assert!(script_pos < close_head_pos);
        // Original markup before <head>'s close is preserved.
        assert!(s.contains("<title>X</title>"));
    }

    #[test]
    fn inject_app_config_escapes_closing_script_tag() {
        // Pretend an app name happens to contain `</script>` — without
        // escaping, the inline JSON would let an HTML parser break out of
        // the script block.
        let mut rt = fake_runtime();
        rt.slug = "evil</script><img src=x>".to_string();
        let html = b"<html><head></head></html>";
        let out = inject_app_config(html, &rt, std::path::Path::new("x.html"));
        let s = std::str::from_utf8(&out).unwrap();
        // The literal `</script>` must NOT appear in the injected payload.
        // (One legitimate `</script>` from the injected script tag itself
        // is allowed.)
        let script_count = s.matches("</script>").count();
        assert_eq!(
            script_count, 1,
            "should be exactly one </script> (the injected tag's own close)"
        );
    }

    #[test]
    fn inject_app_config_no_head_passes_through() {
        let html = b"<html><body>bare</body></html>";
        let out = inject_app_config(html, &fake_runtime(), std::path::Path::new("x.html"));
        assert_eq!(out, html);
    }

    #[test]
    fn find_subsequence_finds_match() {
        assert_eq!(find_subsequence(b"abcdefg", b"cde"), Some(2));
        assert_eq!(find_subsequence(b"abcdefg", b"xyz"), None);
        assert_eq!(find_subsequence(b"", b"x"), None);
    }

    #[test]
    fn prefix_bare_absolute_paths_rewrites_vite_default_output() {
        // Vite's `pnpm build` with no `base:` option produces bare
        // absolute asset paths. Without prefixing them at serve time
        // the browser requests them from the wrong origin and 404s.
        let html = r#"<!doctype html><html><head>
<script type="module" crossorigin src="/assets/index-DA-MTpVz.js"></script>
<link rel="stylesheet" crossorigin href="/assets/style-XXX.css">
</head><body></body></html>"#;
        let out = prefix_bare_absolute_paths(html, "/customer-apps/acme/store-pulse/");
        assert!(
            out.contains("src=\"/customer-apps/acme/store-pulse/assets/index-DA-MTpVz.js\""),
            "expected script src to be prefixed, got: {out}"
        );
        assert!(
            out.contains("href=\"/customer-apps/acme/store-pulse/assets/style-XXX.css\""),
            "expected link href to be prefixed, got: {out}"
        );
    }

    #[test]
    fn prefix_bare_absolute_paths_leaves_protocol_relative_alone() {
        // `//cdn.example.com/x.js` points at a different origin
        // entirely — prefixing it would corrupt the URL.
        let html = r#"<script src="//cdn.example.com/lib.js"></script>"#;
        let out = prefix_bare_absolute_paths(html, "/customer-apps/acme/x/");
        assert_eq!(out, html);
    }

    #[test]
    fn prefix_bare_absolute_paths_leaves_external_urls_alone() {
        let html = r#"<link rel="icon" href="https://acme.com/favicon.ico">"#;
        let out = prefix_bare_absolute_paths(html, "/customer-apps/acme/x/");
        assert_eq!(out, html);
    }

    #[test]
    fn prefix_bare_absolute_paths_leaves_relative_urls_alone() {
        let html = r#"<script src="assets/foo.js"></script>"#;
        let out = prefix_bare_absolute_paths(html, "/customer-apps/acme/x/");
        assert_eq!(out, html);
    }

    #[test]
    fn prefix_bare_absolute_paths_handles_single_quotes() {
        let html = r#"<script src='/main.js'></script>"#;
        let out = prefix_bare_absolute_paths(html, "/customer-apps/acme/x/");
        assert!(out.contains("src='/customer-apps/acme/x/main.js'"));
    }

    #[test]
    fn rewrite_bundle_base_path_prefixes_default_base_bundle() {
        // End-to-end: a Vite-default bundle (no /customer-apps/ in HTML)
        // gets rescued by the prefixing pass.
        let html = br#"<html><head><script src="/assets/main-XYZ.js"></script></head></html>"#;
        let out = rewrite_bundle_base_path(html, "/customer-apps/acme/x/", Uuid::nil());
        let s = std::str::from_utf8(&out).unwrap();
        assert!(s.contains("src=\"/customer-apps/acme/x/assets/main-XYZ.js\""));
    }

    #[test]
    fn rewrite_bundle_base_path_no_op_when_already_correct() {
        // Bundle has the right prefix; nothing should change.
        let html = br#"<html><head><script src="/customer-apps/acme/x/assets/main-XYZ.js"></script></head></html>"#;
        let out = rewrite_bundle_base_path(html, "/customer-apps/acme/x/", Uuid::nil());
        assert_eq!(out, html);
    }

    #[test]
    fn cache_control_marks_hashed_assets_immutable() {
        assert_eq!(
            cache_control_for(
                "/assets/index-DA-MTpVz.js",
                std::path::Path::new("index-DA-MTpVz.js")
            ),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(
            cache_control_for(
                "/_next/static/chunks/abc.js",
                std::path::Path::new("abc.js")
            ),
            "public, max-age=31536000, immutable"
        );
    }

    #[test]
    fn cache_control_html_and_root_files_revalidate() {
        assert_eq!(
            cache_control_for("/index.html", std::path::Path::new("index.html")),
            "no-cache"
        );
        assert_eq!(
            cache_control_for("/favicon.ico", std::path::Path::new("favicon.ico")),
            "public, max-age=300"
        );
    }

    #[test]
    fn etag_is_stable_and_weak() {
        let a = etag_for(b"<html>hello</html>");
        let b = etag_for(b"<html>hello</html>");
        let c = etag_for(b"<html>world</html>");
        assert!(a.starts_with("W/\""), "weak etag, got {a}");
        assert_eq!(a, b, "deterministic");
        assert_ne!(a, c, "content-sensitive");
    }
}
