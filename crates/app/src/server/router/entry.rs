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
use uuid::Uuid;

use crate::agentic_wiring::builder_bridges::OxyBuilderAppRunner;
use crate::api::middlewares::timeout::timeout_middleware;
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
    // Create AgenticState first — the preagg worker needs its db + runtime.
    let agentic_state = new_agentic_state(shutdown_token, true).await?;

    // Spawn the background pre-aggregation refresh worker (Layer 2 freshness).
    // Only when the workspace path is non-empty (i.e. `oxy serve`, not the internal API).
    // The cache Arc is shared with per-request WorkspaceContext via AppState so that
    // Layer 1 (per-query) and Layer 2 (background worker) observe the same entries.
    let (preagg_cache, preagg_renewal_threshold_secs) = if !startup_cwd.as_os_str().is_empty() {
        use crate::agentic_wiring::OxyProjectContext;
        use crate::server::preagg_worker::{PreaggWorkerConfig, spawn_preagg_worker};
        use agentic_semantic::refresh_key_cache::RefreshKeyCache;
        use oxy::adapters::workspace::builder::WorkspaceBuilder;

        // Read pre_aggregations config from config.yml so schema/database/worker
        // settings are driven by the project, not hardcoded defaults.
        let preagg_cfg: Option<oxy::config::model::PreaggConfig> =
            match oxy::config::ConfigBuilder::new().with_workspace_path(&startup_cwd) {
                Ok(b) => b
                    .build_with_fallback_config()
                    .await
                    .ok()
                    .and_then(|cm| cm.get_config().pre_aggregations.clone()),
                Err(_) => None,
            };

        let worker_cfg = preagg_cfg.as_ref().and_then(|p| p.refresh_worker.as_ref());

        let enabled = worker_cfg.and_then(|w| w.enabled).unwrap_or(true);

        if enabled {
            let heartbeat = worker_cfg
                .and_then(|w| w.heartbeat.as_deref())
                .and_then(|s| airlayer::preagg::parse_interval(s).ok())
                .unwrap_or(std::time::Duration::from_secs(30));

            let renewal_threshold = worker_cfg
                .and_then(|w| w.renewal_threshold.as_deref())
                .and_then(|s| airlayer::preagg::parse_interval(s).ok())
                .unwrap_or(std::time::Duration::from_secs(120));

            let schema = preagg_cfg
                .as_ref()
                .and_then(|p| p.schema.clone())
                .unwrap_or_else(|| "AIRLAYER".into());

            let database = preagg_cfg.as_ref().and_then(|p| p.database.clone());

            // Build OxyProjectContext once at startup — shared across all heartbeat ticks.
            let workspace_manager = WorkspaceBuilder::new(Uuid::nil())
                .with_workspace_path_and_fallback_config(&startup_cwd)
                .await
                .map_err(|e| {
                    OxyError::RuntimeError(format!("preagg: workspace builder init failed: {e}"))
                })?
                .build()
                .await
                .map_err(|e| {
                    OxyError::RuntimeError(format!("preagg: workspace build failed: {e}"))
                })?;

            let cache = std::sync::Arc::new(std::sync::RwLock::new(RefreshKeyCache::new()));
            spawn_preagg_worker(
                PreaggWorkerConfig {
                    workspace_path: startup_cwd.clone(),
                    heartbeat,
                    renewal_threshold,
                    schema,
                    database,
                    db: agentic_state.db.clone(),
                    state: agentic_state.runtime.clone(),
                    ctx: std::sync::Arc::new(
                        OxyProjectContext::new(workspace_manager)
                            .with_preagg_renewal_threshold_secs(renewal_threshold.as_secs()),
                    ),
                    manifest_write_lock: std::sync::Arc::new(tokio::sync::Mutex::new(())),
                },
                cache.clone(),
            );
            (Some(cache), Some(renewal_threshold.as_secs()))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    spawn_recovery(agentic_state.clone(), mode);
    spawn_shutdown_hook(agentic_state.clone());
    // Periodic maintenance for customer-app procedure runs: TTL
    // sweep + cross-instance cancel reconciliation + stuck-row
    // recovery. Cheap (delete-many + two update-many every 10 min)
    // and keeps the table bounded under load.
    crate::server::api::projects::procedure_run::spawn_periodic_sweep(
        agentic_state.db.clone(),
        agentic_state.shutdown_token.clone(),
    );

    let app_state = AppState {
        enterprise,
        internal: false,
        mode,
        observability,
        startup_cwd,
        preagg_cache,
        preagg_renewal_threshold_secs,
        agentic_state: Some(agentic_state.clone()),
    };

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
        preagg_cache: None,
        preagg_renewal_threshold_secs: None,
        // Internal router has no customer-app endpoints; agentic state
        // not needed and explicitly omitted.
        agentic_state: None,
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

    // Cross-process wake source. The factory abstracts over both
    // password and IAM auth modes — same OXY_DATABASE_AUTH_MODE env
    // var that selects the connection pool's auth path. IAM mode
    // mints a fresh SigV4 token on every (re)connect via the
    // factory closure; no separate refresh loop needed (the listener
    // holds one connection at a time and Postgres doesn't re-auth
    // mid-stream).
    let factory = oxy::database::client::listener_factory_from_env()
        .map_err(|e| OxyError::RuntimeError(format!("listener factory: {e}")))?;
    let (router_handle, router_cancel) =
        agentic_runtime::router::PostgresTaskRouter::start(db.clone(), factory);
    let router: Arc<dyn agentic_runtime::router::TaskRouter> = router_handle;
    // Tie the listener's lifetime to the same shutdown token as the
    // rest of the agentic state. Dropped CancellationToken would
    // auto-cancel only on the last clone going away; explicit
    // cancel-on-shutdown is clearer.
    let shutdown_for_router = shutdown_token.clone();
    tokio::spawn(async move {
        shutdown_for_router.cancelled().await;
        router_cancel.cancel();
    });
    tracing::info!(
        target: "agentic",
        keepalive_secs = agentic_runtime::router::DEFAULT_LISTENER_KEEPALIVE_INTERVAL.as_secs(),
        "task router: PostgresTaskRouter (LISTEN/NOTIFY)"
    );

    // Process-level background jobs (reaper today; matcher health
    // probe later). Tied to the same shutdown token as the rest of
    // the agentic state so the loop exits on Ctrl-C / SIGTERM with
    // everything else.
    let bg_cancel = agentic_runtime::background::start(db.clone(), router.clone());
    let shutdown_for_bg = shutdown_token.clone();
    tokio::spawn(async move {
        shutdown_for_bg.cancelled().await;
        bg_cancel.cancel();
    });

    Ok(Arc::new(
        AgenticState::new(shutdown_token, db, thread_owner)
            .with_builder_test_runner(Arc::new(OxyTestRunner))
            .with_builder_app_runner(Arc::new(OxyBuilderAppRunner))
            .with_router(router),
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
