//! `oxy worker` — run an agentic worker process standalone.
//!
//! Counterpart to `oxy serve --no-workers`: HTTP frontends accept requests
//! and write to `agentic_task_queue`; one or more worker processes drain
//! the queue via the durable transport's `SELECT ... FOR UPDATE SKIP
//! LOCKED` claim path. The subcommand only adds the long-running process
//! that owns the orchestrator + recovery loop — no HTTP surface beyond the
//! optional k8s probe port.
//!
//! See `internal-docs/worker-fleet.md` for full deployment guidance.
//!
//! ## Database connection
//!
//! Two auth modes are supported (controlled by `OXY_DATABASE_AUTH_MODE`):
//!
//! - **`password`** (default) — set `OXY_DATABASE_URL` to the full
//!   PostgreSQL connection string.
//! - **`iam`** — AWS RDS IAM auth; `OXY_DATABASE_URL` is **not** required.
//!   Set `OXY_DATABASE_HOST`, `OXY_DATABASE_NAME`, `OXY_DATABASE_USER`,
//!   `OXY_DATABASE_REGION`, and optionally `OXY_DATABASE_PORT` /
//!   `OXY_DATABASE_SSL_MODE` instead.
//!
//! Other env vars: `OXY_WORKER_MAX_INFLIGHT`,
//! `OXY_WORKER_RECOVERY_INTERVAL_SECS`, `OXY_WORKER_HEALTH_PORT`.

use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use oxy::database::client::{DatabaseAuthMode, IamConfig};
use oxy::theme::StyledText;
use oxy_shared::errors::OxyError;
use tokio::signal;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::cli::commands::serve;
use crate::server::worker_health::{self, WorkerHealthState};
use crate::server::worker_runtime::WorkerRuntime;

/// Default cadence for the standalone-worker recovery loop. Same default the
/// in-process driver uses (`OXY_INPROC_GLOBAL_WORKER_INTERVAL_SECS`).
const DEFAULT_RECOVERY_INTERVAL_SECS: u64 = 30;
const RECOVERY_INTERVAL_ENV: &str = "OXY_WORKER_RECOVERY_INTERVAL_SECS";
const HEALTH_PORT_ENV: &str = "OXY_WORKER_HEALTH_PORT";

/// Arguments for the `oxy worker` command.
#[derive(Parser, Debug, Clone)]
pub struct WorkerArgs {
    /// Enable enterprise features when running migrations.
    ///
    /// Matches the same `--enterprise` flag on `oxy serve`; tracked separately
    /// so migration code can branch without re-reading the serve args.
    #[clap(long, default_value_t = false)]
    pub enterprise: bool,

    /// Skip running migrations at startup.
    ///
    /// Useful when scaling a fleet behind a leader that already ran them, or
    /// when migrations are run as a separate `oxy migrate` job in CI/CD.
    /// Default OFF — the first standalone worker on a fresh deployment still
    /// runs migrations exactly like `oxy serve` does.
    #[clap(long, default_value_t = false)]
    pub skip_migrations: bool,

    /// Override the recovery loop interval in seconds.
    ///
    /// Falls back to `OXY_WORKER_RECOVERY_INTERVAL_SECS`, then to 30s. The
    /// loop drives `scope_owned = false` runs (scheduler-seeded, crash-
    /// orphaned) on a periodic tick — same engine the in-process global
    /// driver uses inside `oxy serve` today.
    #[clap(long)]
    pub recovery_interval_secs: Option<u64>,

    /// Bind a tiny health server on this TCP port exposing `/healthz` and
    /// `/readyz`. Intended for k8s liveness / readiness probes only — no Oxy
    /// HTTP routes are mounted here.
    ///
    /// Falls back to `OXY_WORKER_HEALTH_PORT`. Default OFF (no port bound).
    #[clap(long)]
    pub health_port: Option<u16>,
}

/// Entry point for `oxy worker`.
///
/// Bootstraps the orchestrator, logs config, then blocks on SIGINT/SIGTERM.
#[tracing::instrument(skip_all, fields(worker_id = tracing::field::Empty, version = tracing::field::Empty))]
pub async fn run_worker(args: WorkerArgs) -> Result<(), OxyError> {
    require_database_url()?;

    if !args.skip_migrations {
        tracing::info!("worker: running database migrations");
        serve::run_database_migrations(args.enterprise).await?;
        tracing::info!("worker: migrations complete");
    } else {
        tracing::info!("worker: --skip-migrations set, leaving migrations to another process");
    }

    let max_inflight = read_max_inflight();
    let recovery_interval = resolve_recovery_interval(args.recovery_interval_secs);
    let health_port = resolve_health_port(args.health_port);
    let db_host = mask_db_url();
    let version = env!("CARGO_PKG_VERSION");
    let commit = option_env!("GITHUB_SHA").unwrap_or("unknown");
    let worker_id = compute_worker_id();

    // Record on the current tracing span so child spans inherit the
    // identifier without re-passing it everywhere.
    let span = tracing::Span::current();
    span.record("worker_id", worker_id.as_str());
    span.record("version", version);

    tracing::info!(
        worker_id = worker_id.as_str(),
        worker_version = version,
        commit = commit,
        max_inflight,
        recovery_interval_secs = recovery_interval.as_secs(),
        health_port = ?health_port,
        db_host = %db_host,
        "worker: starting standalone worker process"
    );
    println!(
        "{} {} {}",
        "Oxy worker starting".text(),
        format!("(v{version}, commit {commit})").secondary(),
        format!("[id={worker_id} max_inflight={max_inflight}]").tertiary(),
    );

    let shutdown = CancellationToken::new();
    let runtime = WorkerRuntime::start(shutdown.clone(), worker_id.clone()).await?;

    let health_state = WorkerHealthState::new(version, worker_id.clone());
    let recovery_alive = health_state.recovery_alive_handle();
    let router_connected = health_state.router_connected_handle();
    // Router is connected once `WorkerRuntime::start` returns; record it.
    router_connected.store(true, std::sync::atomic::Ordering::Relaxed);

    // Recovery loop drives stranded / pending Global runs back to
    // completion via the same PostgresTaskRouter the HTTP server uses, but
    // without HTTP-only concerns (cleanup banners, SSE notifiers).
    //
    // TODO: first cut only runs the reaper pre-pass. Cloud-mode
    // workspace enumeration in the worker process isn't wired up yet —
    // see internal-docs/worker-fleet.md for the punted scope.
    recovery_alive.store(true, std::sync::atomic::Ordering::Relaxed);
    let recovery_handle = spawn_recovery(&runtime, shutdown.clone(), recovery_interval);

    // Optional health probe + Prometheus metrics server (off unless
    // --health-port / OXY_WORKER_HEALTH_PORT is set). Metrics share the
    // same listener as the probes — one tiny axum router with three
    // routes. HPA scrapes `oxy_queue_depth_queued` to size the fleet.
    let health_handle = match health_port {
        Some(port) => {
            let metrics_state = crate::server::worker_metrics::MetricsState {
                worker_id: std::sync::Arc::new(worker_id.clone()),
                version,
                db: std::sync::Arc::new(
                    oxy::database::client::establish_connection()
                        .await
                        .map_err(|e| {
                            OxyError::RuntimeError(format!(
                                "worker metrics: DB connect failed: {e}"
                            ))
                        })?,
                ),
                capacity: crate::server::worker_metrics::ConcurrencyCaps::from_env(),
            };
            Some(
                worker_health::spawn(port, health_state, Some(metrics_state), shutdown.clone())
                    .await?,
            )
        }
        None => None,
    };

    // Block until a shutdown signal arrives, then cascade.
    wait_for_shutdown_signal().await;
    tracing::info!(
        worker_id = worker_id.as_str(),
        "worker: shutdown signal received, cancelling outstanding tasks"
    );
    println!("{}", "Oxy worker shutting down".text());
    shutdown.cancel();
    recovery_alive.store(false, std::sync::atomic::Ordering::Relaxed);

    drain_background(recovery_handle, health_handle).await;

    // Dropping `runtime` releases router + background-job handles.
    drop(runtime);
    tracing::info!(
        worker_id = worker_id.as_str(),
        "worker: graceful shutdown complete"
    );
    println!("{}", "Oxy worker stopped".success());
    Ok(())
}

/// Spawn the periodic recovery loop. Returns the join handle so the caller
/// can drain it on shutdown.
fn spawn_recovery(
    runtime: &WorkerRuntime,
    shutdown: CancellationToken,
    interval: Duration,
) -> JoinHandle<()> {
    let db = runtime.db.clone();
    let router = runtime.router.clone();
    tokio::spawn(async move {
        run_recovery_loop(db, router, shutdown, interval).await;
    })
}

/// Wait for the recovery loop (and optional health server) to drain after
/// the umbrella token is cancelled. We bound the wait so a stuck task can't
/// keep the process alive forever.
async fn drain_background(recovery_handle: JoinHandle<()>, health_handle: Option<JoinHandle<()>>) {
    let drain_deadline = Duration::from_secs(30);
    match tokio::time::timeout(drain_deadline, recovery_handle).await {
        Ok(Ok(())) => tracing::info!("worker: recovery loop drained cleanly"),
        Ok(Err(e)) => tracing::warn!(error = ?e, "worker: recovery loop join failed"),
        Err(_) => tracing::warn!(
            "worker: recovery loop did not drain within {}s; abandoning",
            drain_deadline.as_secs()
        ),
    }
    if let Some(handle) = health_handle {
        match tokio::time::timeout(Duration::from_secs(5), handle).await {
            Ok(Ok(())) => tracing::info!("worker: health server drained cleanly"),
            Ok(Err(e)) => tracing::warn!(error = ?e, "worker: health server join failed"),
            Err(_) => tracing::warn!("worker: health server did not drain within 5s; abandoning"),
        }
    }
}

/// Periodic recovery tick: drive stranded / pending Global runs back to
/// completion. Mirrors the in-process global driver's contract inside
/// `oxy serve`, scoped to local-mode workspace resolution for the first
/// cut. The pure-async entry point already CAS-protects against double-
/// drive across replicas via the driver-lease table, so multiple worker
/// processes are safe to run side-by-side.
#[tracing::instrument(skip_all, fields(interval_secs = interval.as_secs()))]
async fn run_recovery_loop(
    db: sea_orm::DatabaseConnection,
    router: Arc<dyn agentic_runtime::router::TaskRouter>,
    shutdown: CancellationToken,
    interval: Duration,
) {
    tracing::info!(
        target: "worker.recovery",
        interval_secs = interval.as_secs(),
        "recovery loop started"
    );
    // Run one tick up-front so /readyz becoming true means recovery has
    // already executed at least once, not "is scheduled to". Without this,
    // a 30s default interval means /readyz turns true at T+0 but the first
    // reaper pass doesn't fire until T+30.
    tokio::select! {
        _ = shutdown.cancelled() => {
            tracing::info!(target: "worker.recovery", "recovery loop: shutdown before first tick");
            return;
        }
        _ = tick_once(&db, &router, interval) => {}
    }
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::info!(target: "worker.recovery", "recovery loop: shutdown");
                return;
            }
            _ = tokio::time::sleep(interval) => {
                tick_once(&db, &router, interval).await;
            }
        }
    }
}

/// One recovery pass: reaper pre-pass (conditional) + future stranded-run
/// sweep. We deliberately do not build a PlatformContext here yet — the
/// standalone worker first cut relies on the HTTP node to actually drive
/// runs. The reaper alone makes the queue resilient to crashes; the
/// stranded-run sweep is what kicks off background scheduler jobs once we
/// wire up cloud-mode workspace iteration.
///
/// The pre-pass is skipped when the recovery interval is >= the background
/// reaper's own interval — running both at the same cadence is pure churn
/// against `agentic_task_queue`. Short `--recovery-interval-secs` still gets
/// the benefit (e.g. `--recovery-interval-secs 5` for incident triage).
///
/// TODO: integrate `agentic_pipeline::recovery::recover_stranded_runs` with
/// per-workspace PlatformContext construction so this loop actually drives
/// runs from the worker process. For now the reaper pre-pass is the active
/// work and recovery is best-effort.
async fn tick_once(
    db: &sea_orm::DatabaseConnection,
    router: &Arc<dyn agentic_runtime::router::TaskRouter>,
    interval: Duration,
) {
    if interval >= agentic_runtime::background::REAPER_INTERVAL {
        // Background reaper already runs at >= the same cadence; calling
        // it again here just duplicates writes against agentic_task_queue.
        return;
    }
    let transport =
        agentic_runtime::transport::DurableTransport::with_router(db.clone(), router.clone(), None);
    let reaped = transport.run_reaper().await;
    if reaped > 0 {
        tracing::info!(
            target: "worker.recovery",
            count = reaped,
            "reaper pre-pass: freed stale queue entries"
        );
    }
}

/// Validate that the environment has everything needed to open the database
/// connection.  The check is auth-mode-aware:
///
/// - `password` mode (default): `OXY_DATABASE_URL` must be set.
/// - `iam` mode: delegates to [`IamConfig::from_env`] which validates all
///   required vars including port format and ssl-mode values.
///
/// Using the canonical platform validators keeps this guard in sync with
/// the actual connection layer automatically.
fn require_database_url() -> Result<(), OxyError> {
    match DatabaseAuthMode::from_env()? {
        DatabaseAuthMode::Iam => IamConfig::from_env().map(|_| ()),
        DatabaseAuthMode::Password => {
            if std::env::var("OXY_DATABASE_URL").is_err() {
                return Err(OxyError::RuntimeError(
                    "OXY_DATABASE_URL environment variable is required.\n\n\
                     Set it to the same PostgreSQL connection string your `oxy serve`\n\
                     frontend uses — the worker process and the HTTP server must share\n\
                     one queue (and therefore one Postgres database).\n\n\
                     Alternatively, set OXY_DATABASE_AUTH_MODE=iam and supply the\n\
                     individual OXY_DATABASE_HOST / NAME / USER / REGION vars."
                        .to_string(),
                ));
            }
            Ok(())
        }
    }
}

fn read_max_inflight() -> usize {
    std::env::var(agentic_runtime::worker::MAX_INFLIGHT_ENV)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(agentic_runtime::worker::DEFAULT_MAX_INFLIGHT)
}

fn resolve_recovery_interval(arg: Option<u64>) -> Duration {
    let secs = arg
        .or_else(|| {
            std::env::var(RECOVERY_INTERVAL_ENV)
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
        })
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_RECOVERY_INTERVAL_SECS);
    Duration::from_secs(secs)
}

/// Resolve the health probe port: CLI flag wins, then env var, then None.
/// Unparsable env values quietly fall through to `None` instead of
/// crashing the worker on startup — health probes are optional.
fn resolve_health_port(arg: Option<u16>) -> Option<u16> {
    if let Some(port) = arg {
        return Some(port);
    }
    std::env::var(HEALTH_PORT_ENV)
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
}

/// Stable per-process worker identifier used as a structured log field on
/// the startup banner, shutdown logs, and `#[tracing::instrument]` spans.
///
/// Format: `{hostname}@{pid}`. We do not pull in the `hostname` crate; the
/// `HOSTNAME` env var is set on virtually every container runtime (k8s,
/// docker, systemd) and the fallback `"unknown"` is fine for the local
/// dev case. This is an instance identifier, not PII.
fn compute_worker_id() -> String {
    let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string());
    format!("{host}@{}", std::process::id())
}

/// Return a safe-to-log database identifier.
///
/// - `password` mode: strips the password from `OXY_DATABASE_URL`.
/// - `iam` mode: formats `host/database` from the individual IAM vars since
///   there is no URL to parse.
fn mask_db_url() -> String {
    if DatabaseAuthMode::from_env().ok() == Some(DatabaseAuthMode::Iam) {
        let host = std::env::var("OXY_DATABASE_HOST").unwrap_or_else(|_| "<unset>".to_string());
        let db = std::env::var("OXY_DATABASE_NAME").unwrap_or_else(|_| "<unset>".to_string());
        return format!("{host}/{db} (IAM)");
    }
    mask_db_url_str(&std::env::var("OXY_DATABASE_URL").unwrap_or_default())
}

/// Internal helper: pure function over a URL string so tests don't have to
/// mutate process env.
fn mask_db_url_str(raw: &str) -> String {
    match url::Url::parse(raw) {
        Ok(mut u) => {
            // We never log the password — overwrite if present.
            if u.password().is_some() {
                let _ = u.set_password(Some("***"));
            }
            u.to_string()
        }
        Err(_) => "<invalid OXY_DATABASE_URL>".to_string(),
    }
}

async fn wait_for_shutdown_signal() {
    // Force-exit watcher: any second SIGINT arriving while we're draining
    // (or even before the first signal fires) kills the process. Spawning
    // this *before* the wait — rather than inside the same function as a
    // child task that leaks across drains — keeps shutdown reentrant and
    // makes the test-only signal path easier to reason about.
    let _force_exit: JoinHandle<()> = tokio::spawn(async {
        // Wait for the first Ctrl+C to arm the watcher, then wait again for
        // a second one. The first wait deliberately races with the primary
        // shutdown handler below; whichever fires first, the watcher only
        // exits the process on the *second* Ctrl+C.
        if signal::ctrl_c().await.is_err() {
            return;
        }
        if signal::ctrl_c().await.is_err() {
            return;
        }
        tracing::warn!("worker: second shutdown signal, forcing exit");
        std::process::exit(1);
    });

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("worker: SIGINT received");
        }
        _ = terminate => {
            tracing::info!("worker: SIGTERM received");
        }
    }
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
