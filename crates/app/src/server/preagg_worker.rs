//! Background pre-aggregation refresh worker (Layer 2 freshness).
//!
//! Spawned once at server startup. Sleeps for `heartbeat` seconds, then
//! submits a `preagg_cycle` task through the standard agentic job stack
//! (Worker + Coordinator + LocalTransport) so each cycle is persisted and
//! visible in the run history like any other agentic job.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::sync::Mutex as TokioMutex;

use agentic_core::delegation::TaskSpec;
use agentic_core::transport::CoordinatorTransport;
use agentic_runtime::coordinator::Coordinator;
use agentic_runtime::crud;
use agentic_runtime::state::RuntimeState;
use agentic_runtime::transport::LocalTransport;
use agentic_runtime::worker::Worker;
use agentic_semantic::refresh_key_cache::RefreshKeyCache;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use super::preagg_executor::PreaggTaskExecutor;
use crate::agentic_wiring::OxyProjectContext;

/// Configuration for the background refresh worker.
#[derive(Clone)]
pub struct PreaggWorkerConfig {
    /// How often the worker wakes up to check staleness.
    pub heartbeat: Duration,
    /// How long a cached refresh_key result is considered valid.
    pub renewal_threshold: Duration,
    /// Workspace root path (where `.view.yml` files live).
    pub workspace_path: PathBuf,
    /// Schema name for warehouse pre-agg tables.
    pub schema: String,
    /// Database connector override for all preagg builds. When `Some`, overrides each
    /// view's own `datasource`. When `None`, each view uses its own `datasource`.
    pub database: Option<String>,
    /// Shared DB connection for run persistence.
    pub db: DatabaseConnection,
    /// Runtime state for SSE notification.
    pub state: Arc<RuntimeState>,
    /// Project context shared across all heartbeat ticks (built once at startup).
    pub ctx: Arc<OxyProjectContext>,
    /// Serializes manifest writes across parallel rollup rebuilds.
    pub manifest_write_lock: Arc<TokioMutex<()>>,
}

/// Spawn the background pre-aggregation refresh worker.
///
/// Returns immediately. The worker runs until the process exits.
pub fn spawn_preagg_worker(config: PreaggWorkerConfig, cache: Arc<RwLock<RefreshKeyCache>>) {
    let config = Arc::new(config);

    tokio::spawn(async move {
        // Crash recovery: mark any preagg_cycle runs still "running" as failed.
        // These were interrupted by a server restart; the next tick retries from scratch.
        recover_stuck_preagg_runs(&config.db).await;

        let mut ticker = tokio::time::interval(config.heartbeat);
        ticker.tick().await; // skip immediate first tick

        loop {
            ticker.tick().await;
            let config_clone = Arc::clone(&config);
            let cache_clone = Arc::clone(&cache);
            if let Err(e) = tokio::spawn(run_preagg_cycle(config_clone, cache_clone)).await {
                tracing::warn!(error = %e, "preagg cycle task panicked");
            }
        }
    });
}

async fn recover_stuck_preagg_runs(db: &DatabaseConnection) {
    use agentic_runtime::entity::run;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    let stuck = match run::Entity::find()
        .filter(run::Column::SourceType.eq("preagg_cycle"))
        .filter(run::Column::TaskStatus.eq("running"))
        .all(db)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            // Don't silently swallow — a DB error here looks identical to
            // "nothing stuck" and masks real recovery failures.
            tracing::warn!(
                error = %e,
                "preagg recovery: failed to query stuck runs; skipping cycle"
            );
            return;
        }
    };

    for run_model in &stuck {
        if let Err(e) =
            crud::update_run_failed(db, &run_model.id, "interrupted: server restarted").await
        {
            tracing::warn!(
                run_id = %run_model.id,
                error = %e,
                "preagg recovery: failed to mark run failed"
            );
        }
    }

    if !stuck.is_empty() {
        tracing::info!(
            count = stuck.len(),
            "preagg: marked interrupted runs failed on startup"
        );
    }
}

async fn run_preagg_cycle(config: Arc<PreaggWorkerConfig>, cache: Arc<RwLock<RefreshKeyCache>>) {
    let run_id = Uuid::new_v4().to_string();

    if let Err(e) = crud::insert_run(
        &config.db,
        &run_id,
        "preagg: rebuild cycle",
        None,
        "preagg_cycle",
        None,
        config.ctx.workspace_manager().workspace_id,
    )
    .await
    {
        tracing::warn!(error = %e, "preagg cycle: failed to insert run row, skipping");
        return;
    }

    // In-process transport — no shared queue contention with analytics tasks.
    // LocalTransport::with_defaults() already returns Arc<Self>.
    let transport = LocalTransport::with_defaults();

    let executor = Arc::new(PreaggTaskExecutor {
        config: Arc::clone(&config),
        cache: Arc::clone(&cache),
        ctx: Arc::clone(&config.ctx),
    });

    let worker = Worker::new(transport.clone(), executor);

    let coordinator_transport: Arc<dyn CoordinatorTransport> = transport.clone();
    let mut coordinator = Coordinator::new(
        config.db.clone(),
        config.state.clone(),
        coordinator_transport,
    );

    if let Err(e) = coordinator
        .submit_root(
            run_id,
            TaskSpec::Custom {
                kind: "preagg_cycle".into(),
                payload: serde_json::Value::Null,
            },
        )
        .await
    {
        tracing::warn!(error = ?e, "preagg cycle: failed to submit root task");
        return;
    }

    // Drive coordinator to completion. Worker runs in background and is aborted
    // after coordinator finishes — worker.run() blocks until the assignment
    // channel closes, which never happens while transport Arcs are alive, so
    // joining on it would deadlock the outer heartbeat loop.
    let worker_handle = tokio::spawn(async move { worker.run().await });
    coordinator.run().await;
    worker_handle.abort();
}
