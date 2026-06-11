//! Axum middleware that enforces [`role_manifest`] classifications.
//!
//! Two layers:
//!
//! 1. [`enforce_role`] — runs BEFORE the handler. Looks up the matched
//!    path's classification and returns `421 Misdirected Request` with a
//!    `X-Oxy-Required-Role` hint if this process can't serve it.
//! 2. [`stamp_served_by`] — runs AFTER the handler. Adds
//!    `X-Oxy-Served-By: <role>@<worker_id>` so devs can tell who answered
//!    just by reading response headers.
//!
//! `OXY_ROLE` controls behavior:
//! - Unset (default) → `Role::All`, every route is accepted.
//! - `OXY_ROLE=ide`  → only IdeOnly + FleetOk.
//! - `OXY_ROLE=serve` → only FleetOk.

use axum::{
    body::Body,
    extract::{MatchedPath, Request},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::server::role_manifest::{classify, current_process_role, Role, RouteRole};

const HEADER_SERVED_BY: &str = "x-oxy-served-by";
const HEADER_REQUIRED_ROLE: &str = "x-oxy-required-role";

pub async fn enforce_role(req: Request, next: Next) -> Response {
    let role = current_process_role();
    if matches!(role, Role::All) {
        return stamp(next.run(req).await, role);
    }

    let method = req.method().as_str().to_string();
    let matched_path = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_string());

    let Some(matched_path) = matched_path else {
        // No matched path = 404 is coming anyway; let it through.
        return stamp(next.run(req).await, role);
    };

    let route_role = classify(&method, &matched_path);
    if route_role.accepted_by(role) {
        return stamp(next.run(req).await, role);
    }

    let required = required_role_for(route_role);
    tracing::warn!(
        method = %method,
        path = %matched_path,
        process_role = role.as_str(),
        required_role = required,
        "misroute: process role does not accept this route classification"
    );
    let body = format!(
        "this oxy server runs as role '{}'; route '{} {}' is classified '{}' and must be served by role '{}'",
        role.as_str(),
        method,
        matched_path,
        route_role.as_str(),
        required,
    );
    let mut resp = (StatusCode::MISDIRECTED_REQUEST, body).into_response();
    if let Ok(v) = HeaderValue::from_str(required) {
        resp.headers_mut().insert(HEADER_REQUIRED_ROLE, v);
    }
    stamp(resp, role)
}

/// Map a [`RouteRole`] to the canonical process role string used in the
/// `X-Oxy-Required-Role` header.
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

/// `<hostname>#<pid>`. K8s sets HOSTNAME automatically; local dev falls
/// back to "unknown" which is fine for the X-Oxy-Served-By debug header.
fn worker_id() -> String {
    let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string());
    format!("{host}#{}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::Request as HttpRequest;
    use axum::middleware;
    use axum::routing::{get, post};
    use axum::Router;
    use tower::ServiceExt;

    #[tokio::test]
    async fn ide_only_route_on_serve_replica_returns_421() {
        // SAFETY: tests are single-threaded; we restore the env on exit.
        unsafe { std::env::set_var("OXY_ROLE", "serve") };
        crate::server::role_manifest::init_process_role_from_env();

        let app: Router = Router::new()
            .route(
                "/{workspace_id}/compile",
                post(|| async { "should not reach" }),
            )
            .layer(middleware::from_fn(enforce_role));

        let resp = app
            .oneshot(
                HttpRequest::post("/abc/compile")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::MISDIRECTED_REQUEST);
        assert_eq!(
            resp.headers().get(HEADER_REQUIRED_ROLE).unwrap(),
            "ide"
        );
        assert!(resp
            .headers()
            .get(HEADER_SERVED_BY)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("serve@"));

        unsafe { std::env::remove_var("OXY_ROLE") };
    }

    #[tokio::test]
    async fn fleet_ok_route_on_serve_replica_passes_through() {
        unsafe { std::env::set_var("OXY_ROLE", "serve") };
        crate::server::role_manifest::init_process_role_from_env();

        let app: Router = Router::new()
            .route("/healthz", get(|| async { "ok" }))
            .layer(middleware::from_fn(enforce_role));

        let resp = app
            .oneshot(HttpRequest::get("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"ok");

        unsafe { std::env::remove_var("OXY_ROLE") };
    }

    #[tokio::test]
    async fn all_role_accepts_everything() {
        unsafe { std::env::remove_var("OXY_ROLE") };
        crate::server::role_manifest::init_process_role_from_env();

        let app: Router = Router::new()
            .route(
                "/{workspace_id}/compile",
                post(|| async { "handled" }),
            )
            .layer(middleware::from_fn(enforce_role));

        let resp = app
            .oneshot(
                HttpRequest::post("/abc/compile")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp
            .headers()
            .get(HEADER_SERVED_BY)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("all@"));
    }
}
