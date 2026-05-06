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
use tokio_util::sync::CancellationToken;

use crate::api::middlewares::timeout::timeout_middleware;
use crate::server::builder_app_runner::OxyAppRunner;
use crate::server::builder_test_runner::OxyTestRunner;
use crate::server::serve_mode::ServeMode;

use super::protected::{
    apply_local_middleware, apply_middleware, build_local_protected_routes, build_protected_routes,
};
use super::public::build_public_routes;
use super::recovery::{spawn_recovery, spawn_shutdown_hook};
use super::{AppState, build_cors_layer};

pub async fn api_router(
    mode: ServeMode,
    enterprise: bool,
    observability: Option<std::sync::Arc<dyn oxy_observability::ObservabilityStore>>,
    startup_cwd: std::path::PathBuf,
    shutdown_token: CancellationToken,
) -> Result<Router, OxyError> {
    let app_state = AppState {
        enterprise,
        internal: false,
        mode,
        observability,
        startup_cwd,
    };
    let agentic_state = new_agentic_state(shutdown_token, true).await?;
    spawn_recovery(agentic_state.clone(), mode);
    spawn_shutdown_hook(agentic_state.clone());

    let protected_routes = match mode {
        ServeMode::Cloud => {
            // Billing applies to cloud mode only. The reconciliation job no-ops
            // when Stripe isn't configured (STRIPE_SECRET_KEY absent), so the
            // spawn is always safe. Spawning once at router construction keeps
            // it tied to server lifetime without adding a separate hook.
            // Disabled for now; will re-enable later.
            // spawn_billing_reconciler().await;
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

/// 6-hour background loop that reconciles Stripe seat quantity for every
/// paid org, catching any drift between member counts and what we last sent
/// to Stripe. Idempotent; silently does nothing if Stripe isn't configured.
#[allow(dead_code)] // Disabled for now; will re-enable later.
async fn spawn_billing_reconciler() {
    static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if STARTED.set(()).is_err() {
        // Already spawned (e.g. tests or duplicated router builds).
        return;
    }
    let Ok(svc) = crate::api::billing::billing_service().await else {
        tracing::debug!("billing reconciler not spawned — Stripe isn't configured");
        return;
    };
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(6 * 60 * 60));
        // Skip the immediate tick — reconciliation at boot is redundant with
        // the live sync that just fired during any recent member change.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            if let Err(e) = svc.reconcile_all_seats().await {
                tracing::warn!(?e, "billing seat reconciliation failed");
            }
        }
    });
}

pub async fn internal_api_router(
    enterprise: bool,
    observability: Option<std::sync::Arc<dyn oxy_observability::ObservabilityStore>>,
    shutdown_token: CancellationToken,
) -> Result<Router, OxyError> {
    let app_state = AppState {
        enterprise,
        internal: true,
        mode: ServeMode::Cloud,
        observability,
        startup_cwd: std::path::PathBuf::new(),
    };
    // `api_router` owns startup cleanup + recovery for the whole process;
    // the internal router shares the same database state, so it skips both
    // to avoid racing with the primary recovery task on the same runs.
    let agentic_state = new_agentic_state(shutdown_token, false).await?;
    spawn_shutdown_hook(agentic_state.clone());

    let protected_routes = build_protected_routes(app_state.clone(), agentic_state)
        .layer(middleware::from_fn(timeout_middleware))
        .layer(middleware::from_fn(internal_auth_middleware));

    let app_routes = build_public_routes().merge(protected_routes);

    Ok(finalize_router(app_routes, app_state))
}

async fn new_agentic_state(
    shutdown_token: CancellationToken,
    run_cleanup: bool,
) -> Result<Arc<AgenticState>, OxyError> {
    let db = oxy::database::client::establish_connection()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("db connect failed: {e}")))?;
    if run_cleanup {
        cleanup_stale_runs(&db).await.ok();
    }
    let thread_owner: Arc<dyn agentic_pipeline::platform::ThreadOwnerLookup> =
        Arc::new(crate::agentic_wiring::OxyThreadOwnerLookup::new(db.clone()));
    Ok(Arc::new(
        AgenticState::new(shutdown_token, db, thread_owner)
            .with_builder_test_runner(Arc::new(OxyTestRunner))
            .with_builder_app_runner(Arc::new(OxyAppRunner)),
    ))
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
