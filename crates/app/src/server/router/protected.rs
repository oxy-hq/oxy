//! Composes protected (auth-gated) routes for cloud and local modes.
//!
//! Cloud mounts [`build_global_routes`] alongside the workspace tree and
//! applies the standard auth middleware. Local mode omits global routes
//! and swaps in a guest-only auth stack plus the local workspace context.

use std::sync::Arc;

use axum::Router;
use axum::middleware;

use agentic_http::AgenticState;
use oxy_auth::middleware::{AuthState, auth_middleware};
use oxy_shared::errors::OxyError;

use crate::api::middlewares::local_context::local_context_middleware;
use crate::api::middlewares::timeout::timeout_middleware;
use crate::api::middlewares::workspace_context::workspace_middleware;

use super::AppState;
use super::global::build_global_routes;
use super::workspace::build_workspace_routes;

pub(super) fn build_protected_routes(
    app_state: AppState,
    agentic_state: Arc<AgenticState>,
) -> Router<AppState> {
    Router::new().merge(build_global_routes()).nest(
        "/{workspace_id}",
        build_workspace_routes(app_state.clone(), agentic_state, true, false).layer(
            middleware::from_fn_with_state(app_state, workspace_middleware),
        ),
    )
}

pub(super) fn apply_middleware(
    protected_routes: Router<AppState>,
) -> Result<Router<AppState>, OxyError> {
    Ok(protected_routes
        .layer(middleware::from_fn(timeout_middleware))
        .layer(middleware::from_fn_with_state(
            AuthState::built_in(),
            auth_middleware,
        )))
}

/// Local-mode protected routes: mount the same `build_workspace_routes` content
/// surface under `/{workspace_id}` (mirroring the cloud router's URL shape, so
/// existing `Path<WorkspacePath>` extractors still work). The URL segment in
/// local mode is always `LOCAL_WORKSPACE_ID` (nil UUID) — clients hardcode it.
///
/// `build_global_routes` (org + workspace CRUD) is intentionally omitted.
pub(super) fn build_local_protected_routes(
    app_state: AppState,
    agentic_state: Arc<AgenticState>,
) -> Router<AppState> {
    Router::new().nest(
        "/{workspace_id}",
        build_workspace_routes(app_state, agentic_state, false, true)
            .route_layer(middleware::from_fn(local_context_middleware)),
    )
}

pub(super) fn apply_local_middleware(
    protected_routes: Router<AppState>,
) -> Result<Router<AppState>, OxyError> {
    Ok(protected_routes
        .route_layer(middleware::from_fn(timeout_middleware))
        .route_layer(middleware::from_fn_with_state(
            AuthState::guest_only(),
            auth_middleware,
        )))
}
