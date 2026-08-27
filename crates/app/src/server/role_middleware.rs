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
const HEADER_FORWARDED_VIA: &str = "x-oxy-forwarded-via";

pub async fn enforce_role(req: Request, next: Next) -> Response {
    let role = current_process_role();
    if matches!(role, Role::All) {
        return stamp(next.run(req).await, role);
    }

    let method = req.method().as_str().to_string();
    let path = req.uri().path().to_string();
    let route_role = escalate_for_branch(classify(&method, &path), req.uri().query());
    if route_role.accepted_by(role) {
        return stamp(next.run(req).await, role);
    }

    if matches!(role, Role::Serve)
        && matches!(route_role, RouteRole::IdeOnly)
        && let Some(upstream) = crate::server::ide_proxy::ide_upstream()
    {
        if crate::server::ide_proxy::already_forwarded(&req) {
            tracing::error!(
                method = %method,
                path = %path,
                "ide_proxy loop guard: re-forwarded request reached a serve replica — \
                 OXY_IDE_UPSTREAM must target ide-only pods; rejecting"
            );
        } else {
            if crate::server::serve_safety::analytics_fleet_unpin_enabled()
                && let Some(ws) = crate::server::serve_safety::analytics_workspace_id(&path)
                && crate::server::serve_safety::workspace_is_serve_safe(ws).await
            {
                tracing::debug!(
                    workspace_id = %ws,
                    path = %path,
                    "serve replica: serve-safe /analytics handled locally (fleet un-pin)"
                );
                return stamp(next.run(req).await, role);
            }
            tracing::debug!(
                method = %method,
                path = %path,
                "serve replica: forwarding IdeOnly route to ide upstream"
            );
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

fn escalate_for_branch(role: RouteRole, query: Option<&str>) -> RouteRole {
    if matches!(role, RouteRole::IdeOnly) {
        return role;
    }
    let has_branch = query.is_some_and(|q| {
        q.split('&')
            .filter_map(|pair| pair.split_once('='))
            .any(|(k, v)| k == "branch" && !v.is_empty())
    });
    if has_branch { RouteRole::IdeOnly } else { role }
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

fn worker_id() -> String {
    let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string());
    format!("{host}#{}", std::process::id())
}

#[cfg(test)]
#[path = "role_middleware_tests.rs"]
mod tests;
