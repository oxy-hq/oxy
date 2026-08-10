//! Composes protected (auth-gated) routes for cloud and local modes.
//!
//! Cloud mounts [`build_global_routes`] alongside the workspace tree and
//! applies the standard auth middleware. Local mode omits global routes
//! and swaps in a guest-only auth stack plus the local workspace context.

use std::sync::Arc;

use axum::Router;
use axum::http::StatusCode;
use axum::middleware;
use axum::response::{IntoResponse, Response};

use agentic_http::AgenticState;
use oxy_auth::middleware::{AuthState, api_key_only_middleware, auth_middleware};
use oxy_shared::errors::OxyError;

use crate::api::middlewares::api_key_query::api_key_query_middleware;
use crate::api::middlewares::app_publish_token_scope::app_publish_token_scope_middleware;
use crate::api::middlewares::local_context::local_context_middleware;
use crate::api::middlewares::subscription_guard::workspace_subscription_guard_middleware;
use crate::api::middlewares::timeout::timeout_middleware;
use crate::api::middlewares::workspace_context::workspace_middleware;
use oxy_app_core::serve_mode::ServeMode;

use super::AppState;
use super::global::build_global_routes;
use super::workspace::{build_external_workspace_routes, build_workspace_routes};

pub(super) fn build_protected_routes(
    app_state: AppState,
    agentic_state: Arc<AgenticState>,
) -> Router<AppState> {
    Router::new().merge(build_global_routes()).nest(
        "/{workspace_id}",
        build_workspace_routes(app_state.clone(), agentic_state, true, false)
            .layer(middleware::from_fn(workspace_subscription_guard_middleware))
            .layer(middleware::from_fn_with_state(
                app_state,
                workspace_middleware,
            )),
    )
}

pub(super) fn apply_middleware(
    protected_routes: Router<AppState>,
) -> Result<Router<AppState>, OxyError> {
    Ok(protected_routes
        // Innermost: runs AFTER auth has attached identity + any admin-token
        // marker, so it can confine admin-token requests to the customer-apps
        // admin surface before they reach a handler. No-op for cookie/JWT/
        // API-key sessions.
        .layer(middleware::from_fn(app_publish_token_scope_middleware))
        .layer(middleware::from_fn(timeout_middleware))
        .layer(middleware::from_fn_with_state(
            AuthState::built_in(),
            auth_middleware,
        ))
        // Run BEFORE the auth gate so EventSource (SSE) can authenticate
        // via `?api_key=` query param — browsers can't attach headers to
        // EventSource requests. axum applies `.layer` from the outside in,
        // so this declaration places the query-param promoter outermost,
        // which is what we want.
        .layer(middleware::from_fn(api_key_query_middleware)))
}

/// Local-mode protected routes: mount the same `build_workspace_routes` content
/// surface under `/{workspace_id}` (mirroring the cloud router's URL shape, so
/// existing `Path<WorkspacePath>` extractors still work). The URL segment in
/// local mode is always `LOCAL_WORKSPACE_ID` (nil UUID) — clients hardcode it.
///
/// `build_global_routes` (org + workspace CRUD) is intentionally omitted.
/// However the per-user Airhouse routes ARE mounted here too: they're per-user
/// + per-org and don't depend on workspace context. Local mode seeds a
/// nil-UUID org with the local guest user as Owner, so the existing
/// per-org provision flow works untouched.
pub(super) fn build_local_protected_routes(
    app_state: AppState,
    agentic_state: Arc<AgenticState>,
) -> Router<AppState> {
    Router::new()
        .merge(airhouse::api::router::<AppState>())
        .nest(
            "/{workspace_id}",
            build_workspace_routes(app_state.clone(), agentic_state, false, true).route_layer(
                middleware::from_fn_with_state(app_state, local_context_middleware),
            ),
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
        ))
        .route_layer(middleware::from_fn(api_key_query_middleware)))
}

/// Loud JSON 404 for the external API surface. Without an explicit fallback,
/// an unmatched `/external/api/*` path lets its default 404 propagate up to
/// serve.rs's `.fallback_service(main)` and gets the SPA HTML back — which is
/// exactly why a mis-wired external route (the clip-playback gap) surfaced as
/// `undefined` JSON fields in the client instead of a clear 404. An explicit
/// fallback intercepts before the SPA. Auth runs as an outer layer, so an
/// unauthenticated miss still returns 401, not this.
async fn external_api_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({
            "error": "not_found",
            "message": "no such external API route",
        })),
    )
        .into_response()
}

/// Build the EXTERNAL API surface: the curated workspace routes
/// (`build_external_workspace_routes`) under `/{workspace_id}`, gated by
/// API-key-ONLY auth and served with wide-open CORS.
///
/// This is a fully self-contained `Router` (state applied) intended to be
/// mounted at the top level (`/external/api`) *outside* the global
/// `build_cors_layer`, so it carries only its own permissive
/// `build_external_cors_layer`. It reuses the same workspace-resolution
/// middleware as the main surface (so the `{workspace_id}` context + handlers
/// behave identically) but swaps the cookie-accepting `auth_middleware` for
/// `api_key_only_middleware` — that swap is what makes `*`-origin CORS safe
/// (no ambient cookie credential ⇒ no CSRF).
pub(super) fn build_external_api_router(
    app_state: AppState,
    agentic_state: Arc<AgenticState>,
    mode: ServeMode,
) -> Router {
    let curated = build_external_workspace_routes(agentic_state);

    // Resolve the `{workspace_id}` context exactly as the main surface does,
    // per mode. Runs AFTER auth (it needs the authenticated user).
    let with_context = match mode {
        ServeMode::Cloud => curated
            .layer(middleware::from_fn(workspace_subscription_guard_middleware))
            .layer(middleware::from_fn_with_state(
                app_state.clone(),
                workspace_middleware,
            )),
        ServeMode::Local => curated.route_layer(middleware::from_fn_with_state(
            app_state.clone(),
            local_context_middleware,
        )),
    };

    Router::new()
        .nest("/{workspace_id}", with_context)
        // Explicit 404 so an unmatched external path returns JSON, not the SPA
        // HTML it would otherwise fall through to (serve.rs `.fallback_service`).
        // A nested miss propagates its default 404 up to this fallback; auth
        // (below) still runs first, so unauth misses stay 401.
        .fallback(external_api_not_found)
        // Order mirrors `apply_middleware`: api_key_query is OUTERMOST (runs
        // first) so EventSource's `?api_key=` is promoted to the X-API-Key
        // header before the API-key-only auth gate reads it.
        .layer(middleware::from_fn(timeout_middleware))
        .layer(middleware::from_fn(api_key_only_middleware))
        .layer(middleware::from_fn(api_key_query_middleware))
        .layer(super::build_external_cors_layer())
        .with_state(app_state)
}
