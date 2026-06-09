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
    apply_local_middleware, apply_middleware, build_external_api_router,
    build_local_protected_routes, build_protected_routes,
};
use super::public::build_public_routes;
use super::recovery::{spawn_recovery, spawn_shutdown_hook};
use super::{AppState, build_cors_layer};

/// Builds the main API router (mounted under `/api`) and, alongside it, the
/// external API router (`/external/api`). The external one is returned
/// separately so the caller can mount it OUTSIDE the global CORS layer; see
/// [`build_external_api_router`].
pub async fn api_router(
    mode: ServeMode,
    enterprise: bool,
    observability: Option<std::sync::Arc<dyn oxy_observability::ObservabilityStore>>,
    startup_cwd: std::path::PathBuf,
    shutdown_token: CancellationToken,
    disable_inprocess_workers: bool,
) -> Result<(Router, Router), OxyError> {
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

    if disable_inprocess_workers {
        tracing::info!(
            "serve: --no-workers / OXY_DISABLE_INPROCESS_WORKERS active; skipping \
             startup recovery and global driver loop. A separate `oxy worker` \
             fleet must drive the agentic_task_queue."
        );
    } else {
        spawn_recovery(agentic_state.clone(), mode);
    }
    spawn_shutdown_hook(agentic_state.clone());
    // Periodic maintenance for customer-app procedure runs: TTL
    // sweep + cross-instance cancel reconciliation + stuck-row
    // recovery. Cheap (delete-many + two update-many every 10 min)
    // and keeps the table bounded under load.
    crate::server::api::projects::procedure_run::spawn_periodic_sweep(
        agentic_state.db.clone(),
        agentic_state.shutdown_token.clone(),
    );

    // Camera fleet stale-checker: flips edge_boxes.status to 'offline'
    // when last_seen_at goes silent past STALE_THRESHOLD. Bound to the
    // same shutdown token as the rest of the agentic state so it exits
    // cleanly on SIGTERM. The inverse transition (offline → active) is
    // handled by the auth middleware on the next /control/* call from
    // the box, so this loop is one-way only.
    oxy_cameras::service::stale::spawn(
        agentic_state.db.clone(),
        agentic_state.shutdown_token.clone(),
    );

    // Camera fleet log retention sweep (IoT Phase 6a). DELETEs old
    // rows from `oxy_cam_device_logs` per the configured policy
    // (default: info/debug 7d, warn+ 30d). Honors
    // OXY_CAMERA_LOG_SWEEP_INTERVAL_HOURS=0 to disable when
    // operators prefer the `oxy cameras sweep-logs` CLI on cron.
    oxy_cameras::service::log_retention::spawn(
        agentic_state.db.clone(),
        agentic_state.shutdown_token.clone(),
    );

    // OTA rollout supervisor (P1 OTA #3). Advances `camera_rollout_plans`
    // rows through pending → canary → promoting → complete, auto-aborts
    // when canary failure rate exceeds threshold. Same shutdown token
    // as the other camera-domain loops.
    oxy_cameras::service::rollouts::spawn(
        agentic_state.db.clone(),
        agentic_state.shutdown_token.clone(),
    );

    // Camera health → Slack alerter (P1 #2). Polls camera_health
    // summary every 60s per workspace, diffs against the previous
    // tick, emits a Slack message on transitions to/from `ok` with
    // a 30-min per-camera cooldown. No-op when no workspace has a
    // Slack installation.
    oxy_cameras::service::alerts::spawn(
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
            apply_middleware(build_protected_routes(
                app_state.clone(),
                agentic_state.clone(),
            ))?
        }
        ServeMode::Local => apply_local_middleware(build_local_protected_routes(
            app_state.clone(),
            agentic_state.clone(),
        ))?,
    };

    // Camera-fleet routes split across two mounting points:
    //   - Edge /control/* tree mounts here at the top level; the
    //     device-token middleware (inside oxy_cameras::routes::router)
    //     resolves workspace_id from the bearer.
    //   - Operator workspace tree (cameras/edge-boxes, cameras/{id}/zones,
    //     integrations/unifi/*) is merged into build_workspace_routes,
    //     so it sits behind workspace_middleware + auth_middleware and
    //     trusts the URL's workspace_id.
    let camera_routes = oxy_cameras::routes::router::<AppState>(agentic_state.db.clone());
    let app_routes = build_public_routes()
        .merge(protected_routes)
        .merge(camera_routes);

    // External API surface (`/external/api`): curated routes, API-key-only
    // auth, wide-open CORS. Built from the SAME shared `app_state` +
    // `agentic_state` (never a second AgenticState) and returned separately so
    // the caller mounts it OUTSIDE the global `build_cors_layer` — that's what
    // lets its own permissive CORS govern preflight for these routes.
    let external_router = build_external_api_router(app_state.clone(), agentic_state, mode);

    Ok((finalize_router(app_routes, app_state), external_router))
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
    // Match the connection pool's TLS strictness (OXY_DATABASE_SSL_MODE) so
    // the listener's handshake succeeds where the pool's does — RDS and
    // CloudNativePG CAs aren't in the Mozilla bundle.
    let tls_verification = oxy::database::client::listener_tls_verification_from_env()
        .map_err(|e| OxyError::RuntimeError(format!("listener tls mode: {e}")))?;
    let (router_handle, router_cancel) =
        agentic_runtime::router::PostgresTaskRouter::start_with_options(
            db.clone(),
            factory,
            agentic_runtime::router::PostgresTaskRouterOptions {
                tls_verification,
                ..Default::default()
            },
        );
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
