//! HTTP router, split into per-concern modules.
//!
//! - [`public`] — unauthenticated routes (health, auth, Slack, current-user)
//! - [`global`] — cloud-only org/workspace CRUD and GitHub integration
//! - [`workspace`] — the per-workspace route tree and its sub-builders
//! - [`secrets`] — secret routes gated behind an admin-only middleware
//! - [`protected`] — cloud/local composition of protected routes + middleware
//! - [`entry`] — [`api_router`] / [`internal_api_router`] public entry points
//! - [`openapi`] — the utoipa OpenAPI router used by Swagger UI

mod entry;
mod global;
mod openapi;
mod protected;
mod public;
pub(crate) mod recovery;
mod secrets;
mod workspace;
pub(crate) mod workspace_cache;

use crate::server::serve_mode::ServeMode;
use axum::extract::FromRequestParts;
use axum::http::header::{HeaderName, HeaderValue};
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method, StatusCode, header};
use entity::workspaces as workspace_entity;
use std::future::Future;
use tower_http::cors::{AllowOrigin, CorsLayer};

pub use entry::{api_router, internal_api_router};
pub use openapi::openapi_router;

#[derive(Clone)]
pub struct AppState {
    pub enterprise: bool,
    pub internal: bool,
    pub mode: ServeMode,
    pub observability: Option<std::sync::Arc<dyn oxy_observability::ObservabilityStore>>,
    /// The server's working directory at startup. In local mode, used as the
    /// target for `POST /{workspace_id}/setup/*`. In cloud/internal mode,
    /// unused — populated with `PathBuf::new()`.
    pub startup_cwd: std::path::PathBuf,
    /// Shared Layer-1 preagg refresh-key cache. Set when a background preagg
    /// worker is running (i.e. `startup_cwd` is non-empty). `None` in the
    /// internal API router and when no workspace path is configured.
    pub preagg_cache: Option<
        std::sync::Arc<std::sync::RwLock<agentic_semantic::refresh_key_cache::RefreshKeyCache>>,
    >,
    /// Renewal threshold (seconds) for the preagg refresh-key cache.
    /// Mirrors the worker's `pre_aggregations.refresh_worker.renewal_threshold`
    /// so the query read-path uses the operator-configured value, not a
    /// hardcoded default. `None` when no worker is running.
    pub preagg_renewal_threshold_secs: Option<u64>,
    /// Shared agentic state — runtime, schema cache, event registry,
    /// task router. Populated for the main API router so customer-app
    /// endpoints (useAsk, useProcedureRun, useAgentRun) can reach the
    /// pipeline. `None` for the internal API router (no agentic
    /// surface needed there). Handlers should 503 when this is
    /// `None` rather than panic.
    pub agentic_state: Option<std::sync::Arc<agentic_http::AgenticState>>,
    /// Shared per-workspace semantic layer cache. Avoids re-reading and
    /// re-parsing all `.view.yml`/`.topic.yml` files on every request.
    /// Keyed by workspace UUID; TTL of 60 s with explicit invalidation on
    /// semantic file writes.
    pub semantic_layer_cache: std::sync::Arc<workspace_cache::SemanticLayerCache>,
    /// Compiled SemanticEngine cache (join graph + evaluator).
    /// Avoids rebuilding the engine on every compilation request.
    pub semantic_engine_cache: std::sync::Arc<workspace_cache::SemanticEngineCache>,
}

#[derive(Clone)]
pub struct WorkspaceExtractor(pub workspace_entity::Model);

impl<S> FromRequestParts<S> for WorkspaceExtractor
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = parts
            .extensions
            .get::<workspace_entity::Model>()
            .cloned()
            .map(WorkspaceExtractor)
            .ok_or(StatusCode::UNAUTHORIZED);

        async move { result }
    }
}

/// CORS policy for the API surface.
///
/// Customer-app bundles share the SPA's domain in the current model, so
/// real cross-origin browser XHR only happens in local dev (Vite on
/// `:5173` / `:5174` calling oxy on `:3000` when not proxied). Auto-allow
/// that pair plus any same-origin request derived from `Host` /
/// `X-Forwarded-Host`. No env-var setup needed for local or cloud
/// deployments. (When whitelabelling lands, per-app allowed origins
/// belong in the DB, not a global env var.)
pub(crate) fn build_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_private_network(true)
        .allow_origin(AllowOrigin::predicate(|origin, parts| {
            cors_allow(origin, &parts.headers)
        }))
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
            Method::HEAD,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::ACCEPT_LANGUAGE,
            header::ORIGIN,
            HeaderName::from_static("x-requested-with"),
            HeaderName::from_static("x-request-id"),
            // Cross-origin SSE for the customer-app shell's Ask Oxygen stream.
            // Its fetch sends `Cache-Control: no-cache` (don't cache the stream)
            // and, on reconnect, `Last-Event-ID` (replay from a known point).
            // Neither is CORS-safelisted, so each triggers a preflight; without
            // them here the browser blocks the request and the run reports
            // "Failed to fetch". Same headers the external CORS layer allows.
            header::CACHE_CONTROL,
            HeaderName::from_static("last-event-id"),
        ])
}

/// Wide-open CORS for the EXTERNAL API surface (`/external/api/*`).
///
/// Allows ANY origin so a standalone app on any domain (e.g. a Vercel
/// dashboard) can call oxy with `X-API-Key`. This is safe ONLY because that
/// surface is gated by [`oxy_auth::middleware::api_key_only_middleware`] —
/// API-key auth carries no ambient browser credential, so `*`-origin has no
/// CSRF vector. Critically we must NOT set `allow_credentials(true)` here: the
/// CORS spec forbids credentials with a wildcard origin, and there are no
/// cookies to send anyway. Kept entirely separate from [`build_cors_layer`]
/// (the locked-down, cookie-credentialed policy for the main `/api` surface).
pub(crate) fn build_external_cors_layer() -> CorsLayer {
    use tower_http::cors::Any;
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
            Method::HEAD,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::ACCEPT_LANGUAGE,
            header::ORIGIN,
            HeaderName::from_static("x-requested-with"),
            HeaderName::from_static("x-request-id"),
            HeaderName::from_static("x-api-key"),
            HeaderName::from_static("last-event-id"),
        ])
}

/// Shared predicate body for both `build_cors_layer` (browser CORS preflight)
/// and [`is_allowed_origin`] (server-side gate). Both auto-allow either a
/// canonical local-dev origin or the request's own host.
fn cors_allow(origin: &HeaderValue, headers: &HeaderMap) -> bool {
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    is_dev_origin(origin) || is_self_origin(origin, headers)
}

/// Canonical local-dev origins — Vite's web-app (`:5173`) and stand-alone
/// bundle dev server (`:5174`), plus the `:3000`–`:3005` range used by
/// customer-app dev servers (e.g. the Command Center on `:3005`). Always
/// allowed so engineers don't need to configure anything to iterate locally.
fn is_dev_origin(origin: &str) -> bool {
    matches!(
        origin,
        "http://localhost:5173"
            | "http://localhost:5174"
            | "http://localhost:3000"
            | "http://localhost:3001"
            | "http://localhost:3002"
            | "http://localhost:3003"
            | "http://localhost:3004"
            | "http://localhost:3005"
    )
}

/// Return `true` when `origin` (e.g. `https://app.oxygen-hq.com`) targets the
/// same host as the incoming request. We prefer `X-Forwarded-Host` (set by a
/// TLS-terminating reverse proxy) and fall back to `Host`; this covers both
/// direct-bind and behind-a-load-balancer deployments without an env var.
fn is_self_origin(origin: &str, headers: &HeaderMap) -> bool {
    let origin_host = match origin.split_once("://") {
        Some((_, rest)) => rest.trim_end_matches('/'),
        None => return false,
    };
    if origin_host.is_empty() {
        return false;
    }
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(header::HOST))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    !host.is_empty() && origin_host == host
}

/// Check whether the `Origin` (or `Referer`) header on an incoming request
/// targets a permitted origin. Mirrors the CORS layer's predicate as a
/// belt-and-braces server-side check used by sensitive endpoints (the
/// customer-app data gate, etc.).
///
/// Rules:
///   - No `Origin` and no `Referer` → allowed (programmatic clients
///     legitimately omit both).
///   - Origin matches the request's `Host` / `X-Forwarded-Host` → allowed
///     (the standard same-domain deployment).
///   - Origin is one of the canonical Vite dev origins (`:5173`, `:5174`)
///     → allowed (Vite cross-origin proxy in local dev).
///   - Otherwise → rejected.
///
/// If only `Referer` is present, its scheme+host is extracted and used as
/// the candidate origin.
pub(crate) fn is_allowed_origin(headers: &HeaderMap) -> bool {
    let candidate = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .or_else(|| {
            headers
                .get(header::REFERER)
                .and_then(|v| v.to_str().ok())
                .and_then(|r| {
                    // Extract scheme://host[:port] from the Referer URL.
                    let without_scheme = r.find("://").map(|i| &r[i + 3..])?;
                    let host_end = without_scheme.find('/').unwrap_or(without_scheme.len());
                    let host = &without_scheme[..host_end];
                    let scheme = &r[..r.find("://").unwrap()];
                    Some(format!("{scheme}://{host}"))
                })
        });
    match candidate {
        // No Origin or Referer → allow non-browser clients.
        None => true,
        Some(origin) => is_dev_origin(&origin) || is_self_origin(&origin, headers),
    }
}

#[cfg(test)]
mod app_state_tests {
    use super::*;

    #[test]
    fn app_state_carries_mode() {
        let local = AppState {
            enterprise: false,
            internal: false,
            mode: ServeMode::Local,
            observability: None,
            startup_cwd: std::path::PathBuf::from("/tmp"),
            preagg_cache: None,
            preagg_renewal_threshold_secs: None,
            agentic_state: None,
            semantic_layer_cache: super::workspace_cache::new_semantic_layer_cache(),
            semantic_engine_cache: super::workspace_cache::new_semantic_engine_cache(),
        };
        let cloud = AppState {
            enterprise: false,
            internal: false,
            mode: ServeMode::Cloud,
            observability: None,
            startup_cwd: std::path::PathBuf::new(),
            preagg_cache: None,
            preagg_renewal_threshold_secs: None,
            agentic_state: None,
            semantic_layer_cache: super::workspace_cache::new_semantic_layer_cache(),
            semantic_engine_cache: super::workspace_cache::new_semantic_engine_cache(),
        };
        assert!(local.mode.is_local());
        assert!(!cloud.mode.is_local());
    }
}

#[cfg(test)]
mod router_split_tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    /// `api_router()` wires database-backed middleware during construction.
    /// Skip the inline router tests when `OXY_DATABASE_URL` is unset so CI
    /// without a Postgres doesn't flag a config gap as a code regression.
    /// (The DB-gated integration tests in `crates/app/tests/` follow the
    /// same convention.)
    fn db_unavailable() -> bool {
        std::env::var("OXY_DATABASE_URL").is_err()
    }

    #[tokio::test]
    async fn local_router_does_not_expose_organizations() {
        if db_unavailable() {
            return;
        }
        let (router, _external_router) = api_router(
            ServeMode::Local,
            false,
            None,
            std::path::PathBuf::new(),
            tokio_util::sync::CancellationToken::new(),
            false,
        )
        .await
        .expect("router built");
        let req = Request::builder().uri("/orgs").body(Body::empty()).unwrap();
        let resp = router.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "local mode must not mount /orgs"
        );
    }

    #[tokio::test]
    async fn local_router_serves_health() {
        if db_unavailable() {
            return;
        }
        let (router, _external_router) = api_router(
            ServeMode::Local,
            false,
            None,
            std::path::PathBuf::new(),
            tokio_util::sync::CancellationToken::new(),
            false,
        )
        .await
        .expect("router built");
        // /live always returns 200 regardless of DB availability — confirms
        // that public routes are mounted on the local router.
        let req = Request::builder().uri("/live").body(Body::empty()).unwrap();
        let resp = router.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn local_router_mounts_workspace_routes_under_nil_uuid() {
        if db_unavailable() {
            return;
        }
        use crate::server::serve_mode::LOCAL_WORKSPACE_ID;
        let (router, _external_router) = api_router(
            ServeMode::Local,
            false,
            None,
            std::path::PathBuf::new(),
            tokio_util::sync::CancellationToken::new(),
            false,
        )
        .await
        .expect("router built");
        let uri = format!("/{}/agents", LOCAL_WORKSPACE_ID);
        let req = Request::builder().uri(&uri).body(Body::empty()).unwrap();
        let resp = router.oneshot(req).await.expect("oneshot");
        assert_ne!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "local mode must mount workspace routes under /{{workspace_id}}, got {} for {}",
            resp.status(),
            uri
        );
    }

    #[tokio::test]
    async fn cloud_router_still_has_organizations_mounted() {
        if db_unavailable() {
            return;
        }
        let (router, _external_router) = api_router(
            ServeMode::Cloud,
            false,
            None,
            std::path::PathBuf::new(),
            tokio_util::sync::CancellationToken::new(),
            false,
        )
        .await
        .expect("router built");
        let req = Request::builder().uri("/orgs").body(Body::empty()).unwrap();
        let resp = router.oneshot(req).await.expect("oneshot");
        // Route is mounted → request reaches auth/handler, not the router's 404.
        assert_ne!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "cloud mode must keep /orgs mounted, got {}",
            resp.status()
        );
    }
}

#[cfg(test)]
mod cors_tests {
    use super::*;

    fn make_headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut m = HeaderMap::new();
        for (k, v) in pairs {
            m.insert(
                axum::http::header::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        m
    }

    #[test]
    fn no_origin_no_referer_allows_non_browser_clients() {
        // Programmatic clients (CLIs, server-to-server) legitimately omit both.
        assert!(is_allowed_origin(&HeaderMap::new()));
    }

    #[test]
    fn dev_origin_localhost_5173_is_allowed() {
        let headers = make_headers(&[("origin", "http://localhost:5173")]);
        assert!(is_allowed_origin(&headers));
    }

    #[test]
    fn dev_origin_localhost_5174_is_allowed() {
        let headers = make_headers(&[("origin", "http://localhost:5174")]);
        assert!(is_allowed_origin(&headers));
    }

    #[test]
    fn dev_origin_localhost_3005_is_allowed() {
        // Customer-app dev servers run on the :3000–:3005 range (e.g. the
        // Command Center on :3005) and must reach the main `/api` cross-origin.
        let headers = make_headers(&[("origin", "http://localhost:3005")]);
        assert!(is_allowed_origin(&headers));
    }

    #[test]
    fn self_origin_via_host_is_allowed() {
        // Standard same-domain deployment.
        let headers = make_headers(&[
            ("origin", "https://app-dev.oxygen-hq.com"),
            ("host", "app-dev.oxygen-hq.com"),
        ]);
        assert!(is_allowed_origin(&headers));
    }

    #[test]
    fn self_origin_via_x_forwarded_host_is_allowed() {
        // Behind a TLS-terminating reverse proxy, `Host` is the internal
        // address; the public hostname comes in on `X-Forwarded-Host`.
        let headers = make_headers(&[
            ("origin", "https://app.oxygen-hq.com"),
            ("host", "oxy-internal.svc.cluster.local"),
            ("x-forwarded-host", "app.oxygen-hq.com"),
        ]);
        assert!(is_allowed_origin(&headers));
    }

    #[test]
    fn referer_scheme_host_used_when_origin_absent() {
        let headers = make_headers(&[
            ("referer", "https://app.oxygen-hq.com/some/path?q=1"),
            ("host", "app.oxygen-hq.com"),
        ]);
        assert!(is_allowed_origin(&headers));
    }

    #[test]
    fn cross_origin_outside_dev_or_self_is_rejected() {
        // An origin that isn't a canonical dev host and doesn't match the
        // server's own host is rejected — defence-in-depth carries over from
        // the old env-driven allowlist.
        let headers = make_headers(&[
            ("origin", "https://attacker.com"),
            ("host", "app.oxygen-hq.com"),
        ]);
        assert!(!is_allowed_origin(&headers));
    }
}
