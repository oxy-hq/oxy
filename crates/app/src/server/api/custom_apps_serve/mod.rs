//! Subpath bundle serving for custom apps.
//!
//! Serves custom-app frontends at `GET /customer-apps/{uuid}` and
//! `GET /customer-apps/{uuid}/{*rest}`, gating each request with cookie or
//! bearer auth and an org-membership check.
//!
//! Bundle source per app — see `custom_apps_source::AppSource`:
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

use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use entity::prelude::Apps;
use oxy::database::client::establish_connection;
use oxy_auth::authenticator::Authenticator;
use oxy_auth::built_in::BuiltInAuthenticator;
use oxy_auth::user::UserService;
use sea_orm::ColumnTrait;
use sea_orm::EntityTrait;
use sea_orm::QueryFilter;
use uuid::Uuid;

use super::custom_apps_auth::user_can_access_app;
use super::custom_apps_cache::{cached_user, set_cached_user};

mod headers;
mod rewrite;
mod sources;

use headers::*;
use sources::*;

pub(crate) use rewrite::first_custom_apps_prefix;
pub(crate) use sources::serve_from_s3_build;

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
        let body_bytes =
            match axum::body::to_bytes(body, super::custom_apps_proxy::REQUEST_BODY_LIMIT).await {
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
        return super::custom_apps_functions::handle_function_request(
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
pub(crate) async fn serve_pretty(
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
                target: "custom_apps_serve",
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
                tracing::error!("Failed to load user for custom app {org_slug}/{app_slug}: {e}");
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
            tracing::error!("DB connection failed serving custom app {org_slug}/{app_slug}: {e}");
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
            tracing::error!("Failed to look up custom app {org_slug}/{app_slug}: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let id = app.id;

    // Combined access check (org member | workspace grant | global app
    // admin). Cached per (user_id, app_id) for 60s — see
    // `custom_apps_auth::user_can_access_app`. Critical for the Next.js
    // asset storm (30-100 requests per page load).
    let allowed = match user_can_access_app(&db, user.id, &user.email, &app).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Access check failed for custom app {id}: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    if !allowed {
        return (
            StatusCode::FORBIDDEN,
            "You don't have access to this custom app.",
        )
            .into_response();
    }

    // 3. Dispatch through the source facade. The per-app source decision
    //    (v0 / local / s3) is recorded at register time; this handler only
    //    knows about three rendering modes and matches on them.
    let source = match super::custom_apps_source::AppSource::from_model(&app) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Bad source config for custom app {id}: {e}");
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
    let source_label = match super::custom_apps_host_dispatch::parse_subdomain(
        headers
            .get(axum::http::header::HOST)
            .and_then(|v| v.to_str().ok())
            .unwrap_or(""),
    ) {
        Some(_) => "subdomain",
        None => "subpath",
    };

    use super::custom_apps_source::AppSource;
    let response = match source {
        AppSource::V0 { url } => {
            super::custom_apps_proxy::proxy(
                &url,
                &rest,
                method,
                &uri,
                &headers,
                body,
                super::custom_apps_proxy::ProxyIdentity {
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
            let cookie_wants_draft = super::custom_apps_preview::wants_draft_preview(&headers);
            // Fail-closed inside the one reader: a lookup error reports no standing.
            // Admin OR owner — both operator tiers reach every custom-app surface.
            let is_staff = crate::server::authz::globals::platform_standing(&db, &user.email)
                .await
                .is_staff();
            let channel =
                resolve_channel(cookie_wants_draft && is_staff, app.published_at.is_some());
            // New publish pipeline: when the channel has a build pointer,
            // serve straight from S3 (no local state dir). Legacy `s3`
            // rows leave both pointers NULL and fall through to the
            // state-dir path until they're re-published.
            use super::custom_apps_sync::Channel;
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
            super::custom_apps_tracking::session_id_for_serve(&headers, secure);
        let referrer = headers
            .get(axum::http::header::REFERER)
            .and_then(|v| v.to_str().ok());
        let sanitized_referrer = super::custom_apps_tracking::sanitize_referrer(referrer, host);
        let user_agent_class = classify_user_agent(headers.get(axum::http::header::USER_AGENT));
        let app_id = id;
        let user_id = user.id;
        let user_email = user.email.clone();
        let source_label = source_label.to_string();
        // Fire-and-forget; a slow DB insert must not stall the HTML
        // response. Losing a row on crash is the documented acceptable
        // failure mode for tracking-grade data.
        tokio::spawn(async move {
            super::custom_apps_tracking::record_view(
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
