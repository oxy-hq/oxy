//! Process-level background jobs that the agentic runtime needs to
//! keep healthy.
//!
//! Three responsibilities, deliberately co-located in a single task so
//! we don't accumulate background slots over time:
//!
//! 1. **Reaper.** Periodically calls
//!    [`crate::orchestrator::crud::reap_stale_tasks`] to free tasks
//!    claimed by workers that died without cleanup (OOM, panic,
//!    network partition, hung kernel). Without this, a single dead
//!    worker permanently strands every task it held until the next
//!    server restart triggers
//!    [`crate::orchestrator::transport::DurableTransport::run_reaper`]
//!    as a one-shot pre-pass — which is what the system *used* to
//!    rely on exclusively, and which silently lost work between
//!    restarts.
//!
//! 2. **Matcher health probe.** Periodically fires a self-NOTIFY on
//!    [`crate::orchestrator::router::HEALTH_PROBE_CHANNEL`] so every listener (on
//!    this instance and any peer) can observe that the LISTEN/NOTIFY
//!    pipeline is end-to-end functional. Catches the failure mode
//!    where the connection looks healthy (keepalive succeeds) but
//!    server-side notification delivery has stalled. The router
//!    records `last_probe_received_at` on every receipt; alerting
//!    monitors absence of `router.health_probe_received` events.
//!
//! ## Why one task, not three
//!
//! All cadences are slow (reaper ~30s, probe ~60s) and none are
//! latency-sensitive. Splitting them into separate tokio tasks would
//! buy nothing and add extra `CancellationToken`s to pass around at
//! shutdown. A single `select!` loop with a couple of tickers is
//! simpler to reason about and stop cleanly.

use std::sync::Arc;
use std::time::Duration;

use sea_orm::DatabaseConnection;
use tokio_util::sync::CancellationToken;

use crate::crud;
use crate::orchestrator::router::TaskRouter;

/// How often the reaper scans `agentic_task_queue` for stale claims.
///
/// 30s is the recovery RTO — a worker that dies takes at most one
/// `visibility_timeout_secs` (60s default) + one reaper interval
/// before its tasks reappear as `queued`. Total worst case: ~90s
/// from worker death to another worker picking up the dropped task.
/// Tightening to 5s would shave that to ~65s at the cost of 6× more
/// queries; not worth it for the failure mode (worker crashes are
/// rare in steady state).
pub const REAPER_INTERVAL: Duration = Duration::from_secs(30);

/// How often we self-NOTIFY on the matcher health-probe channel.
///
/// 60s is the alert resolution: monitors typically alarm on three
/// missed probes (~3 min of silence), which is short enough to catch
/// a stuck listener in time to bounce the pod, long enough that a
/// minute-long blip during failover doesn't page anyone.
pub const HEALTH_PROBE_INTERVAL: Duration = Duration::from_secs(60);

/// Tunables for [`start_with_options`]. Production uses [`Default`];
/// tests shrink the intervals so probe / reap behaviour can be
/// exercised inside a fast suite.
#[derive(Debug, Clone)]
pub struct BackgroundJobsOptions {
    pub reaper_interval: Duration,
    pub health_probe_interval: Duration,
}

impl Default for BackgroundJobsOptions {
    fn default() -> Self {
        Self {
            reaper_interval: REAPER_INTERVAL,
            health_probe_interval: HEALTH_PROBE_INTERVAL,
        }
    }
}

/// Spawn the process-level background jobs.
///
/// Call this once at app startup, after migrations have run. Holds
/// the returned [`CancellationToken`] for the lifetime of the
/// process; cancel it on shutdown to stop the loop cleanly.
pub fn start(db: DatabaseConnection, router: Arc<dyn TaskRouter>) -> CancellationToken {
    start_with_options(db, router, BackgroundJobsOptions::default())
}

/// Like [`start`] but with configurable intervals. Test-facing.
pub fn start_with_options(
    db: DatabaseConnection,
    router: Arc<dyn TaskRouter>,
    options: BackgroundJobsOptions,
) -> CancellationToken {
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();

    tracing::info!(
        target: "background",
        reaper_interval_secs = options.reaper_interval.as_secs(),
        health_probe_interval_secs = options.health_probe_interval.as_secs(),
        "background jobs started: reaper + health probe"
    );

    tokio::spawn(async move {
        let mut reaper_tick = tokio::time::interval(options.reaper_interval);
        // `Skip` semantics: if the loop falls behind, don't fire
        // queued ticks back-to-back. A reaper is liveness-driven, not
        // count-driven.
        reaper_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut probe_tick = tokio::time::interval(options.health_probe_interval);
        // Same `Skip` semantics + drain the always-immediate first
        // tick. The probe's value is "X seconds since last seen", so
        // an at-startup probe is misleading (no listeners on peer
        // instances might be up yet anyway). Let one interval elapse
        // before the first emission so the signal is meaningful.
        probe_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        probe_tick.tick().await;

        loop {
            tokio::select! {
                _ = task_cancel.cancelled() => {
                    tracing::debug!(target: "background", "background jobs cancelled");
                    return;
                }
                _ = reaper_tick.tick() => {
                    run_reaper_cycle(&db).await;
                }
                _ = probe_tick.tick() => {
                    router.emit_health_probe().await;
                }
            }
        }
    });

    cancel
}

async fn run_reaper_cycle(db: &DatabaseConnection) {
    match crud::reap_stale_tasks(db).await {
        Ok(0) => {
            // Quiet on the happy path — running every 30s, we'd
            // flood the logs with "reaper cycle: 0 0" otherwise.
        }
        Ok(n) => {
            tracing::info!(
                target: "background",
                count = n,
                "reaper cycle: requeued or dead-lettered stale tasks"
            );
        }
        Err(e) => {
            tracing::warn!(
                target: "background",
                error = %e,
                "reaper cycle failed; will retry on next interval"
            );
        }
    }
}
