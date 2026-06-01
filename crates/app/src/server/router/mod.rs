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
mod recovery;
mod secrets;
mod workspace;
mod workspace_cache;

use crate::server::serve_mode::ServeMode;
use axum::extract::FromRequestParts;
use axum::http::header::{HeaderName, HeaderValue};
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method, StatusCode, header};
use entity::workspaces as workspace_entity;
use std::future::Future;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

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
/// The SDK adds `Authorization: Bearer <api-key>` to most requests
/// (see `sdk/typescript/src/client.ts`), and the browser also forwards
/// any `oxy_session` cookie cross-origin under `credentials: "include"`.
/// Both make the request **credentialed**, and the CORS spec requires:
///   - `Access-Control-Allow-Origin` is an exact origin (not `*`)
///   - `Access-Control-Allow-Credentials: true`
/// when the response should be readable. Our previous policy used
/// `allow_origin(Any)` with no `allow_credentials`, which the browser
/// rejected for any cross-origin XHR carrying creds — typically
/// surfaced as "CORS error" from the customer-app bundle iframe at
/// :5173 calling oxy at :3000.
///
/// `OXY_ALLOWED_ORIGINS` (comma-separated) lets ops override the
/// default in prod. Default in local mode is the canonical Vite dev
/// origins (:5173, :5174) which cover both the web-app and a
/// stand-alone bundle `pnpm dev`.
pub(crate) fn build_cors_layer() -> CorsLayer {
    let resolved = resolve_cors_origins();
    let layer = CorsLayer::new().allow_private_network(true);

    match resolved {
        CorsOrigins::Any => {
            // Fully-open. `Allow-Credentials` is intentionally omitted
            // so the spec accepts the `*` origin (browsers reject a
            // credentialed cross-origin response that pairs creds with
            // `*`). Used only when ops opts in via
            // `OXY_ALLOWED_ORIGINS=*` — typically test harnesses.
            layer
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
        }
        CorsOrigins::Explicit(origins) => {
            // Credentialed cross-origin XHR is the dominant case for
            // the SDK (Authorization header) + customer-app bundles
            // (oxy_session cookie). The CORS spec disallows `*` for
            // origin / methods / headers when Allow-Credentials is on,
            // so each list is enumerated explicitly. The header list
            // mirrors what the SDK and Axios actually send today
            // (Authorization, Content-Type, X-Requested-With) plus
            // standard CORS-safelisted headers; widen as more clients
            // appear rather than reverting to `Any`.
            layer
                .allow_origin(AllowOrigin::list(origins))
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
                ])
        }
    }
}

enum CorsOrigins {
    Any,
    Explicit(Vec<HeaderValue>),
}

/// Decide CORS origin policy from `OXY_ALLOWED_ORIGINS`:
///   - unset → default list of canonical dev origins (5173 / 5174)
///   - `*`   → `CorsOrigins::Any` (no credentials)
///   - comma-separated list → explicit allowlist (with credentials)
///
/// Production should set this to the real app host(s) so cross-origin
/// XHR from any other origin is refused.
fn resolve_cors_origins() -> CorsOrigins {
    let raw = std::env::var("OXY_ALLOWED_ORIGINS").ok();
    resolve_cors_origins_from(raw.as_deref())
}

/// Pure inner implementation; accepts the raw env value so unit tests can
/// exercise all branches without mutating the process environment.
fn resolve_cors_origins_from(raw: Option<&str>) -> CorsOrigins {
    let trimmed = raw.map(str::trim);
    match trimmed {
        Some("*") => CorsOrigins::Any,
        Some(s) if !s.is_empty() => {
            let parsed: Vec<HeaderValue> = s
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .filter_map(|s| HeaderValue::from_str(s).ok())
                .collect();
            CorsOrigins::Explicit(parsed)
        }
        _ => {
            // Local-dev defaults: web-app Vite (`:5173`) + stand-alone
            // bundle dev server (`:5174`, per the Vite template).
            let defaults = ["http://localhost:5173", "http://localhost:5174"]
                .into_iter()
                .filter_map(|s| HeaderValue::from_str(s).ok())
                .collect();
            CorsOrigins::Explicit(defaults)
        }
    }
}

/// Check whether the `Origin` (or `Referer`) header on an incoming request is
/// within the configured CORS allowlist.
///
/// Used by sensitive endpoints (e.g. the query proxy) as a defence against
/// same-host bundle-vs-bundle CSRF: a low-vetted bundle script running in an
/// iframe at `bundle-a.example.com` should not be able to call the query
/// endpoint scoped to a project the visiting user owns in a different org.
///
/// Rules:
///   - If `OXY_ALLOWED_ORIGINS=*`, all origins are allowed (ops opt-in for
///     fully-open deployments such as test harnesses).
///   - If `Origin` is absent AND `Referer` is absent, the request is allowed —
///     programmatic clients (CLIs, server-to-server) legitimately omit both.
///   - Otherwise the `Origin` header value must be in the allowlist. If only
///     `Referer` is present (no `Origin`), its scheme+host is extracted and
///     checked.
pub(crate) fn is_allowed_origin(headers: &HeaderMap) -> bool {
    is_allowed_origin_for(headers, &resolve_cors_origins())
}

/// Pure inner implementation; accepts a pre-resolved `CorsOrigins` so unit
/// tests can exercise all branches without mutating the process environment.
fn is_allowed_origin_for(headers: &HeaderMap, origins: &CorsOrigins) -> bool {
    match origins {
        CorsOrigins::Any => true,
        CorsOrigins::Explicit(allowed) => {
            // Prefer the `Origin` header; fall back to scheme+host of `Referer`.
            let candidate = headers
                .get(header::ORIGIN)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_owned())
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
                Some(origin) => {
                    let origin_val = HeaderValue::from_str(&origin).ok();
                    origin_val.is_some_and(|v| allowed.contains(&v))
                }
            }
        }
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
        let router = api_router(
            ServeMode::Local,
            false,
            None,
            std::path::PathBuf::new(),
            tokio_util::sync::CancellationToken::new(),
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
        let router = api_router(
            ServeMode::Local,
            false,
            None,
            std::path::PathBuf::new(),
            tokio_util::sync::CancellationToken::new(),
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
        let router = api_router(
            ServeMode::Local,
            false,
            None,
            std::path::PathBuf::new(),
            tokio_util::sync::CancellationToken::new(),
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
        let router = api_router(
            ServeMode::Cloud,
            false,
            None,
            std::path::PathBuf::new(),
            tokio_util::sync::CancellationToken::new(),
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

    // ── resolve_cors_origins_from ─────────────────────────────────────────

    #[test]
    fn resolve_unset_returns_localhost_defaults() {
        let origins = resolve_cors_origins_from(None);
        let CorsOrigins::Explicit(list) = origins else {
            panic!("expected Explicit, got Any");
        };
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|v| v == "http://localhost:5173"));
        assert!(list.iter().any(|v| v == "http://localhost:5174"));
    }

    #[test]
    fn resolve_star_returns_any() {
        let origins = resolve_cors_origins_from(Some("*"));
        assert!(matches!(origins, CorsOrigins::Any));
    }

    #[test]
    fn resolve_single_origin() {
        let origins = resolve_cors_origins_from(Some("https://app.oxy.tech"));
        let CorsOrigins::Explicit(list) = origins else {
            panic!("expected Explicit");
        };
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], "https://app.oxy.tech");
    }

    #[test]
    fn resolve_comma_separated_origins() {
        let origins =
            resolve_cors_origins_from(Some("https://app.oxy.tech,https://staging.oxy.tech"));
        let CorsOrigins::Explicit(list) = origins else {
            panic!("expected Explicit");
        };
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|v| v == "https://app.oxy.tech"));
        assert!(list.iter().any(|v| v == "https://staging.oxy.tech"));
    }

    #[test]
    fn resolve_invalid_entries_dropped_valid_survive() {
        // HeaderValue::from_str rejects values containing non-ASCII / control
        // characters (e.g. a raw DEL byte). Valid-but-non-URL strings like
        // "foo" are kept because HeaderValue accepts any printable ASCII.
        // Use a NUL-containing entry to guarantee rejection.
        let raw = "https://app.oxy.tech,bad\x00value";
        let origins = resolve_cors_origins_from(Some(raw));
        let CorsOrigins::Explicit(list) = origins else {
            panic!("expected Explicit");
        };
        // Only the valid origin survives; the NUL-bearing entry is dropped.
        assert_eq!(list.len(), 1, "invalid entries must be silently dropped");
        assert_eq!(list[0], "https://app.oxy.tech");
    }

    // ── is_allowed_origin_for ─────────────────────────────────────────────

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

    fn explicit(origins: &[&str]) -> CorsOrigins {
        CorsOrigins::Explicit(
            origins
                .iter()
                .map(|s| HeaderValue::from_str(s).unwrap())
                .collect(),
        )
    }

    #[test]
    fn no_origin_no_referer_allows_non_browser_clients() {
        let headers = HeaderMap::new();
        let cors = explicit(&["https://app.oxy.tech"]);
        assert!(is_allowed_origin_for(&headers, &cors));
    }

    #[test]
    fn origin_in_allowlist_is_allowed() {
        let headers = make_headers(&[("origin", "https://app.oxy.tech")]);
        let cors = explicit(&["https://app.oxy.tech"]);
        assert!(is_allowed_origin_for(&headers, &cors));
    }

    #[test]
    fn origin_not_in_allowlist_is_rejected() {
        let headers = make_headers(&[("origin", "https://attacker.com")]);
        let cors = explicit(&["https://app.oxy.tech"]);
        assert!(!is_allowed_origin_for(&headers, &cors));
    }

    #[test]
    fn referer_scheme_host_matched_when_origin_absent() {
        let headers = make_headers(&[("referer", "https://app.oxy.tech/some/path?q=1")]);
        let cors = explicit(&["https://app.oxy.tech"]);
        assert!(is_allowed_origin_for(&headers, &cors));
    }

    #[test]
    fn cors_any_allows_all_origins() {
        let headers = make_headers(&[("origin", "https://attacker.com")]);
        assert!(is_allowed_origin_for(&headers, &CorsOrigins::Any));
    }
}
