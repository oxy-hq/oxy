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

    let required = required_role_for(route_role);
    tracing::warn!(
        method = %method,
        path = %path,
        process_role = role.as_str(),
        required_role = required,
        "misroute: process role does not accept this route"
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
