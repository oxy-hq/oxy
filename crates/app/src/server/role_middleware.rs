//! Axum middleware enforcing [`role_manifest`] classifications.
//!
//! Runs before the handler. Looks up the request's role and returns
//! `421 Misdirected Request` with `X-Oxy-Required-Role` when this process
//! can't serve it. Always stamps `X-Oxy-Served-By: <role>@<host>#<pid>` so
//! `curl -i` reveals who answered.
//!
//! `OXY_ROLE` controls behavior. Unset (default) → [`Role::All`], every
//! route accepted, behavior unchanged.
//!
//! Classification matches the request's actual URI path (not `MatchedPath`),
//! because the middleware is attached to the outermost router but the real
//! routes live two `nest()` levels down — axum's `MatchedPath` at the outer
//! layer carries only the nest's registration pattern, not the resolved leaf.

use axum::{
    body::Body,
    extract::Request,
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::server::role_manifest::{Role, RouteRole, classify, current_process_role};

const HEADER_SERVED_BY: &str = "x-oxy-served-by";
const HEADER_REQUIRED_ROLE: &str = "x-oxy-required-role";
/// Records the serve proxy hop on a forwarded IdeOnly response (the upstream's
/// `X-Oxy-Served-By` is preserved as who actually answered).
const HEADER_FORWARDED_VIA: &str = "x-oxy-forwarded-via";

pub async fn enforce_role(req: Request, next: Next) -> Response {
    let role = current_process_role();
    if matches!(role, Role::All) {
        return stamp(next.run(req).await, role);
    }

    let method = req.method().as_str().to_string();
    let path = req.uri().path().to_string();
    let route_role = classify(&method, &path);
    if route_role.accepted_by(role) {
        return stamp(next.run(req).await, role);
    }

    // Self-routing: a SERVE replica forwards an `IdeOnly` route to the ide pod
    // instead of rejecting it, when an ide upstream is wired (`OXY_IDE_UPSTREAM`).
    // `role_manifest` is then the routing AUTHORITY — the edge LB round-robins
    // and there is no external ingress route table to drift from the code (the
    // cause of three prior outages). With no upstream (local / single instance /
    // not-yet-wired fleet) we keep the legacy 421, so behaviour is unchanged.
    //
    // Gated on `Role::Serve` specifically: a Worker process also fails
    // `accepted_by` for IdeOnly, but a worker must NEVER act as an IDE proxy
    // (no customer HTTP surface) — it keeps the 421 path.
    //
    // RESIDUAL RISK (tracked fast-follow): `classify` still defaults an
    // *unlisted* route to FleetOk, so a NEW IdeOnly route added to the router
    // without a manifest entry would be served locally and fail. The durable
    // fixes are (a) inverting the per-workspace default to forward-on-doubt and
    // (b) a router-introspecting drift test — see internal-docs. So this is the
    // routing authority, not yet a drift-proof guarantee.
    if matches!(role, Role::Serve)
        && matches!(route_role, RouteRole::IdeOnly)
        && let Some(upstream) = crate::server::ide_proxy::ide_upstream()
    {
        if crate::server::ide_proxy::already_forwarded(&req) {
            // A request we already forwarded came back to a serve replica — the
            // OXY_IDE_UPSTREAM Service is (mis)selecting serve pods. Break the
            // loop with a 421 (fall through) rather than forward a second time.
            tracing::error!(
                method = %method,
                path = %path,
                "ide_proxy loop guard: re-forwarded request reached a serve replica — \
                 OXY_IDE_UPSTREAM must target ide-only pods; rejecting"
            );
        } else if crate::server::role_manifest::degrades_when_ide_unreachable(&method, &path) {
            // Read-only git STATE (GET /details, /status): forward for the live
            // value, but if the ide is UNREACHABLE serve it LOCALLY instead of
            // 502ing — the handler degrades to `git_mode: None` (git ops shown
            // unavailable), so a dead ide never takes the workspace page down.
            // `forward_to_ide_opt` hands the request back (extensions intact) on
            // unreachable, so we fall through to the local handler.
            match crate::server::ide_proxy::forward_to_ide_opt(upstream, req).await {
                Ok(resp) => return stamp_forwarded_via(resp, role),
                Err(mut rebuilt) => {
                    tracing::info!(
                        method = %method,
                        path = %path,
                        "ide unreachable — serving degradable git-state route locally (graceful HA)"
                    );
                    // Mark forwarded so workspace_middleware's fail-safe fallback
                    // doesn't try to re-forward this to the (down) ide.
                    rebuilt.headers_mut().insert(
                        "x-oxy-forwarded-by",
                        HeaderValue::from_static("serve-degraded"),
                    );
                    return stamp(next.run(rebuilt).await, role);
                }
            }
        } else {
            tracing::debug!(
                method = %method,
                path = %path,
                "serve replica: forwarding IdeOnly route to ide upstream"
            );
            // Do NOT re-stamp X-Oxy-Served-By: the upstream response already
            // carries the ide pod's `ide@...` (who actually answered). Record
            // the serve proxy hop separately so both are visible in `curl -i`.
            return stamp_forwarded_via(
                crate::server::ide_proxy::forward_to_ide(upstream, req).await,
                role,
            );
        }
    }

    let required = required_role_for(route_role);
    tracing::warn!(
        method = %method,
        path = %path,
        process_role = role.as_str(),
        required_role = required,
        "misroute: process role does not accept this route (no ide upstream to forward to)"
    );
    let body = format!(
        "this oxy server runs as role '{}'; route '{} {}' is classified '{}' and must be served by role '{}'",
        role.as_str(),
        method,
        path,
        route_role.as_str(),
        required,
    );
    let mut resp = (StatusCode::MISDIRECTED_REQUEST, body).into_response();
    if let Ok(v) = HeaderValue::from_str(required) {
        resp.headers_mut().insert(HEADER_REQUIRED_ROLE, v);
    }
    stamp(resp, role)
}

fn required_role_for(route_role: RouteRole) -> &'static str {
    match route_role {
        RouteRole::IdeOnly => "ide",
        RouteRole::FleetOk => "serve",
        RouteRole::WorkerOnly => "worker",
    }
}

fn stamp(mut resp: Response<Body>, role: Role) -> Response<Body> {
    let header = format!("{}@{}", role.as_str(), worker_id());
    if let Ok(v) = HeaderValue::from_str(&header) {
        resp.headers_mut().insert(HEADER_SERVED_BY, v);
    }
    resp
}

/// Stamp a FORWARDED response. Records the serve proxy hop in
/// `X-Oxy-Forwarded-Via`. On the success path the upstream (ide) already set
/// `X-Oxy-Served-By` to who actually answered, so we preserve it; only when it's
/// absent (the upstream-unreachable 502, which serve generated itself) do we
/// stamp `serve@...` — so every forwarded response, success or failure, says who
/// answered.
fn stamp_forwarded_via(mut resp: Response<Body>, role: Role) -> Response<Body> {
    let Ok(v) = HeaderValue::from_str(&format!("{}@{}", role.as_str(), worker_id())) else {
        return resp;
    };
    resp.headers_mut().insert(HEADER_FORWARDED_VIA, v.clone());
    if !resp.headers().contains_key(HEADER_SERVED_BY) {
        resp.headers_mut().insert(HEADER_SERVED_BY, v);
    }
    resp
}

/// `<hostname>#<pid>`. K8s sets HOSTNAME; local dev falls back to "unknown".
fn worker_id() -> String {
    let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string());
    format!("{host}#{}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::to_bytes;
    use axum::http::Request as HttpRequest;
    use axum::middleware;
    use axum::routing::{get, post};
    use tower::ServiceExt;

    /// Production topology mirror: `/api` outer nest + `/{workspace_id}` inner
    /// nest. The previous tests registered routes flat and missed the
    /// MatchedPath bug entirely. Asserts the middleware classifies a deeply
    /// nested route against the real URI.
    fn nested_router() -> Router {
        let workspace_routes = Router::new()
            .route("/compile", post(|| async { "should not reach" }))
            .route("/threads", get(|| async { "threads ok" }));
        let api_routes = Router::new().nest("/{workspace_id}", workspace_routes);
        Router::new()
            .route("/health", get(|| async { "ok" }))
            .nest("/api", api_routes)
            .layer(middleware::from_fn(enforce_role))
    }

    #[tokio::test]
    async fn ide_only_route_on_serve_replica_returns_421_through_nest() {
        unsafe { std::env::set_var("OXY_ROLE", "serve") };
        crate::server::role_manifest::init_process_role_from_env();

        let resp = nested_router()
            .oneshot(
                HttpRequest::post("/api/some-uuid/compile")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::MISDIRECTED_REQUEST);
        assert_eq!(resp.headers().get(HEADER_REQUIRED_ROLE).unwrap(), "ide");
        assert!(
            resp.headers()
                .get(HEADER_SERVED_BY)
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("serve@")
        );

        unsafe { std::env::remove_var("OXY_ROLE") };
    }

    /// A WORKER process must never act as an IDE proxy, even with an ide
    /// upstream configured. The forward is gated on `Role::Serve`; without that
    /// gate a worker would reverse-proxy IdeOnly traffic and this would 502
    /// against the bogus upstream instead of 421.
    #[tokio::test]
    async fn worker_with_upstream_does_not_forward_ide_route() {
        unsafe {
            std::env::set_var("OXY_ROLE", "worker");
            std::env::set_var("OXY_IDE_UPSTREAM", "http://ide.invalid:80");
        }
        crate::server::role_manifest::init_process_role_from_env();

        let resp = nested_router()
            .oneshot(
                HttpRequest::post("/api/some-uuid/compile")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::MISDIRECTED_REQUEST);
        unsafe {
            std::env::remove_var("OXY_ROLE");
            std::env::remove_var("OXY_IDE_UPSTREAM");
        }
    }

    /// Loop guard: a request already marked forwarded that lands back on a
    /// serve replica (an OXY_IDE_UPSTREAM Service mistakenly selecting serve
    /// pods) must break with 421 — not forward a second time (which would 502
    /// against the bogus upstream).
    #[tokio::test]
    async fn already_forwarded_ide_route_on_serve_breaks_loop() {
        unsafe {
            std::env::set_var("OXY_ROLE", "serve");
            std::env::set_var("OXY_IDE_UPSTREAM", "http://ide.invalid:80");
        }
        crate::server::role_manifest::init_process_role_from_env();

        let resp = nested_router()
            .oneshot(
                HttpRequest::post("/api/some-uuid/compile")
                    .header("x-oxy-forwarded-by", "serve")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::MISDIRECTED_REQUEST);
        unsafe {
            std::env::remove_var("OXY_ROLE");
            std::env::remove_var("OXY_IDE_UPSTREAM");
        }
    }

    #[tokio::test]
    async fn fleet_ok_route_on_serve_replica_passes_through_nest() {
        unsafe { std::env::set_var("OXY_ROLE", "serve") };
        crate::server::role_manifest::init_process_role_from_env();

        let resp = nested_router()
            .oneshot(
                HttpRequest::get("/api/some-uuid/threads")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"threads ok");

        unsafe { std::env::remove_var("OXY_ROLE") };
    }

    #[tokio::test]
    async fn health_probe_passes_on_every_role_including_worker() {
        for role in ["ide", "serve", "worker"] {
            unsafe { std::env::set_var("OXY_ROLE", role) };
            crate::server::role_manifest::init_process_role_from_env();
            let resp = nested_router()
                .oneshot(HttpRequest::get("/health").body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::OK,
                "health probe failed under OXY_ROLE={role}"
            );
        }
        unsafe { std::env::remove_var("OXY_ROLE") };
    }

    #[tokio::test]
    async fn all_role_accepts_everything() {
        unsafe { std::env::remove_var("OXY_ROLE") };
        crate::server::role_manifest::init_process_role_from_env();

        let resp = nested_router()
            .oneshot(
                HttpRequest::post("/api/some-uuid/compile")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Compile handler returns "should not reach" string + 200 in the
        // test router. Under Role::All the middleware just passes through.
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            resp.headers()
                .get(HEADER_SERVED_BY)
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("all@")
        );
    }

    /// Regression: `/ide*` is served by the static `fallback_service` on
    /// the outer router. enforce_role must sit OUTSIDE the API nest so it
    /// covers the fallback path too — if it's only on the inner api router,
    /// IDE pages slip through and OXY_ROLE=serve fails to block them.
    #[tokio::test]
    async fn ide_static_fallback_returns_421_on_serve() {
        unsafe { std::env::set_var("OXY_ROLE", "serve") };
        crate::server::role_manifest::init_process_role_from_env();

        // Mirror production: API tree as inner nest, static-style fallback
        // as a catch-all at the outer level, enforce_role at the outer
        // level (where serve.rs::main now wraps it).
        let api_routes = Router::new().nest(
            "/{workspace_id}",
            Router::new().route("/compile", post(|| async { "" })),
        );
        let app = Router::new()
            .nest("/api", api_routes)
            .fallback(get(|| async { "static html" }))
            .layer(middleware::from_fn(enforce_role));

        // /ide — IdeOnly per manifest — must 421
        let resp = app
            .clone()
            .oneshot(HttpRequest::get("/ide").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::MISDIRECTED_REQUEST,
            "/ide on serve replica should 421"
        );

        // /ide/files/cGF0aA — also IdeOnly
        let resp = app
            .oneshot(
                HttpRequest::get("/ide/files/cGF0aA")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::MISDIRECTED_REQUEST,
            "/ide/{{rest}} on serve replica should 421"
        );

        unsafe { std::env::remove_var("OXY_ROLE") };
    }
}
