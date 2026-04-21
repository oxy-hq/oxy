//! Public entry points that assemble the full router.
//!
//! [`api_router`] is the user-facing API (cloud or local, driven by
//! [`ServeMode`]). [`internal_api_router`] is the internal port — always
//! cloud-shape, protected by an API-key-only middleware.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use sentry::integrations::tower::NewSentryLayer;
use tower::ServiceBuilder;
use tower_http::timeout::TimeoutLayer;

use agentic_http::{AgenticState, cleanup_stale_runs};
use oxy_auth::middleware::internal_auth_middleware;
use oxy_shared::errors::OxyError;

use crate::api::middlewares::timeout::timeout_middleware;
use crate::server::builder_test_runner::OxyTestRunner;
use crate::server::serve_mode::ServeMode;

use super::protected::{
    apply_local_middleware, apply_middleware, build_local_protected_routes, build_protected_routes,
};
use super::public::build_public_routes;
use super::{AppState, build_cors_layer};

pub async fn api_router(
    mode: ServeMode,
    enterprise: bool,
    observability: Option<std::sync::Arc<dyn oxy_observability::ObservabilityStore>>,
    startup_cwd: std::path::PathBuf,
) -> Result<Router, OxyError> {
    let app_state = AppState {
        enterprise,
        internal: false,
        mode,
        observability,
        startup_cwd,
    };
    cleanup_stale_runs().await.ok();
    let agentic_state = new_agentic_state();

    let protected_routes = match mode {
        ServeMode::Cloud => {
            apply_middleware(build_protected_routes(app_state.clone(), agentic_state))?
        }
        ServeMode::Local => apply_local_middleware(build_local_protected_routes(
            app_state.clone(),
            agentic_state,
        ))?,
    };
    let app_routes = build_public_routes().merge(protected_routes);

    Ok(finalize_router(app_routes, app_state))
}

pub async fn internal_api_router(
    enterprise: bool,
    observability: Option<std::sync::Arc<dyn oxy_observability::ObservabilityStore>>,
) -> Result<Router, OxyError> {
    let app_state = AppState {
        enterprise,
        internal: true,
        mode: ServeMode::Cloud,
        observability,
        startup_cwd: std::path::PathBuf::new(),
    };
    cleanup_stale_runs().await.ok();
    let agentic_state = new_agentic_state();

    let protected_routes = build_protected_routes(app_state.clone(), agentic_state)
        .layer(middleware::from_fn(timeout_middleware))
        .layer(middleware::from_fn(internal_auth_middleware));

    let app_routes = build_public_routes().merge(protected_routes);

    Ok(finalize_router(app_routes, app_state))
}

fn new_agentic_state() -> Arc<AgenticState> {
    Arc::new(AgenticState::new().with_builder_test_runner(Arc::new(OxyTestRunner)))
}

/// Applies the shared outer layers — state, CORS, the 60-second global
/// timeout (aligned with load-balancer limits; individual sync endpoints
/// have their own tighter timeouts), and Sentry request tracing.
fn finalize_router(app_routes: Router<AppState>, app_state: AppState) -> Router {
    let global_timeout =
        TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(60));

    app_routes
        .with_state(app_state)
        .layer(build_cors_layer())
        .layer(global_timeout)
        .layer(ServiceBuilder::new().layer(NewSentryLayer::<Request<Body>>::new_from_top()))
}
