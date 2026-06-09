//! Shared bootstrap for the agentic worker plumbing.
//!
//! Both `oxy serve` (in-process worker mode) and `oxy worker` (standalone
//! worker fleet mode) need the same orchestrator pieces wired up:
//!
//! 1. A connected `DatabaseConnection`
//! 2. A `PostgresTaskRouter` started (LISTEN/NOTIFY wake source)
//! 3. The process-level background jobs (reaper + matcher health probe)
//!
//! Splitting that wiring out of `router::entry::new_agentic_state` lets the
//! `oxy worker` subcommand spin up the orchestrator without booting an Axum
//! server. The `oxy serve` path keeps its existing flow — `new_agentic_state`
//! already does exactly this work inline; this module is the standalone-worker
//! counterpart.
//!
//! TODO: once we trust this in production, fold `new_agentic_state` onto this
//! helper so HTTP and standalone workers share one bootstrap path.

use std::sync::Arc;

use agentic_runtime::background;
use agentic_runtime::router::{
    DEFAULT_LISTENER_KEEPALIVE_INTERVAL, PostgresTaskRouter, PostgresTaskRouterOptions, TaskRouter,
};
use oxy::database::client::{
    establish_connection, listener_factory_from_env, listener_tls_verification_from_env,
};
use oxy_shared::errors::OxyError;
use sea_orm::DatabaseConnection;
use tokio_util::sync::CancellationToken;

/// Long-lived handles owned by an `oxy worker` process.
///
/// Drop on shutdown to stop the reaper / health probe and let the router
/// listener disconnect cleanly.
pub struct WorkerRuntime {
    pub db: DatabaseConnection,
    pub router: Arc<dyn TaskRouter>,
    /// Cancel to stop the LISTEN/NOTIFY listener loop.
    pub router_cancel: CancellationToken,
    /// Cancel to stop the reaper + health-probe background tasks.
    pub background_cancel: CancellationToken,
}

impl WorkerRuntime {
    /// Bootstrap the orchestrator: connect to PG, start the task router,
    /// and spawn the reaper + matcher health probe.
    ///
    /// `shutdown` is the umbrella token; when it's cancelled the helper
    /// cascades cancellation to the router listener and background jobs.
    /// `worker_id` is a stable per-process identifier threaded onto the
    /// startup span and the router log line so a multi-replica fleet can be
    /// sliced by instance in logs.
    ///
    /// Returns once the router is ready to accept work.
    #[tracing::instrument(skip_all, fields(worker_id = %worker_id))]
    pub async fn start(shutdown: CancellationToken, worker_id: String) -> Result<Self, OxyError> {
        let db = establish_connection()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("db connect failed: {e}")))?;

        // Same listener factory pattern `oxy serve` uses — handles password
        // and IAM auth modes (`OXY_DATABASE_AUTH_MODE`) without per-mode
        // branching here.
        let factory = listener_factory_from_env()
            .map_err(|e| OxyError::RuntimeError(format!("listener factory: {e}")))?;
        // Match the connection pool's TLS strictness (OXY_DATABASE_SSL_MODE)
        // so the listener's handshake succeeds where the pool's does — RDS
        // and CloudNativePG CAs aren't in the Mozilla bundle.
        let tls_verification = listener_tls_verification_from_env()
            .map_err(|e| OxyError::RuntimeError(format!("listener tls mode: {e}")))?;
        let (router_handle, router_cancel) = PostgresTaskRouter::start_with_options(
            db.clone(),
            factory,
            PostgresTaskRouterOptions {
                tls_verification,
                ..Default::default()
            },
        );
        let router: Arc<dyn TaskRouter> = router_handle;
        tracing::info!(
            target: "agentic",
            worker_id = %worker_id,
            keepalive_secs = DEFAULT_LISTENER_KEEPALIVE_INTERVAL.as_secs(),
            "task router: PostgresTaskRouter (LISTEN/NOTIFY)"
        );

        // Reaper (~30s) + matcher health probe (~60s). Tied to its own cancel
        // token so we can stop it cleanly without dropping the router yet.
        let background_cancel = background::start(db.clone(), router.clone());

        // Cascade umbrella cancellation to both subsystems.
        let cascade_router = router_cancel.clone();
        let cascade_bg = background_cancel.clone();
        let umbrella = shutdown.clone();
        tokio::spawn(async move {
            umbrella.cancelled().await;
            cascade_router.cancel();
            cascade_bg.cancel();
        });

        Ok(Self {
            db,
            router,
            router_cancel,
            background_cancel,
        })
    }
}
