//! Periodic tick driver — runs `scan_workspace` on a fixed cadence per
//! workspace.
//!
//! Lives outside `agentic_pipeline::scheduler` because that scheduler is
//! `target_kind`-driven (workflow / airway) and dispatches via
//! `&dyn WorkflowWorkspaceContext`. The metric-monitoring scan needs a
//! `MetricTreeRunner` which only `PlatformContext` exposes, so we run as a
//! sibling tick alongside the scheduler.
//!
//! Cadence is enforced in-process via a `LastScanRegistry` keyed by
//! `workspace_id`. On process restart every workspace gets one immediate
//! scan (the registry starts empty), then settles into the configured
//! interval. For durable cross-process scheduling, point a real cron at
//! the `/semantic/anomalies/scan` endpoint instead.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};

use agentic_analytics::MetricTreeRunner;
use chrono::Utc;
use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;
use uuid::Uuid;

use std::sync::OnceLock;

use crate::config::default_config_path;
use crate::persist::upsert_anomalies;
use crate::service::{ScanError, scan_workspace};

/// Process-global registry used by the recovery-loop tick path. Lets
/// callers avoid threading a `LastScanRegistry` through every recovery
/// signature; tests and direct API use can still construct their own
/// registries.
static GLOBAL_REGISTRY: OnceLock<LastScanRegistry> = OnceLock::new();

/// Returns the process-global [`LastScanRegistry`], constructing it on
/// first access.
pub fn global_registry() -> &'static LastScanRegistry {
    GLOBAL_REGISTRY.get_or_init(LastScanRegistry::new)
}

/// Tracks when each workspace was last scanned so the tick can be invoked
/// at any cadence (the registry enforces the actual scan interval).
/// Clone-shared: the recovery loop holds one instance for the lifetime of
/// the process.
#[derive(Debug, Default, Clone)]
pub struct LastScanRegistry {
    inner: Arc<Mutex<HashMap<Uuid, Instant>>>,
}

impl LastScanRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if `workspace_id` is due for a scan (never scanned or
    /// `min_interval` elapsed since the last attempt). Updates the timestamp
    /// optimistically — callers that find work to do should treat a `true`
    /// return as a soft lease.
    pub async fn take_if_due(&self, workspace_id: Uuid, min_interval: StdDuration) -> bool {
        let mut guard = self.inner.lock().await;
        let now = Instant::now();
        let due = match guard.get(&workspace_id) {
            Some(last) => now.duration_since(*last) >= min_interval,
            None => true,
        };
        if due {
            guard.insert(workspace_id, now);
        }
        due
    }

    /// Clear the entry for `workspace_id` so the next tick re-runs
    /// immediately. Useful after a manual scan reset.
    pub async fn forget(&self, workspace_id: Uuid) {
        self.inner.lock().await.remove(&workspace_id);
    }
}

/// Outcome of a single workspace tick.
#[derive(Debug)]
pub struct TickOutcome {
    pub workspace_id: Uuid,
    /// `true` if the scan actually ran; `false` if the registry deferred it.
    pub ran: bool,
    /// Count of successful monitors. 0 when `ran=false` or when the workspace
    /// has no `.monitor.yml`.
    pub monitors_scanned: usize,
    /// Count of monitors that errored during the scan.
    pub monitors_failed: usize,
    /// Count of anomaly rows upserted (inserts + updates combined).
    pub anomalies_persisted: usize,
}

/// Failure modes a tick can hit before reaching scan errors. Per-monitor
/// failures inside the scan don't bubble here — they're collected into
/// [`crate::service::ScanResult::failures`] which a caller can inspect.
#[derive(Debug, thiserror::Error)]
pub enum TickError {
    #[error("scan failed: {0}")]
    Scan(#[from] ScanError),
    #[error("persist failed: {0}")]
    Persist(#[from] sea_orm::DbErr),
}

/// Scan one workspace if it's due. Returns a [`TickOutcome`] describing what
/// happened so the caller (recovery loop) can log/aggregate.
///
/// `workspace_root` is the on-disk root of the workspace; the function
/// looks for `<root>/.monitor.yml` and returns early if it doesn't exist.
pub async fn tick_workspace(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    workspace_root: &Path,
    runner: Arc<dyn MetricTreeRunner>,
    registry: &LastScanRegistry,
    min_interval: StdDuration,
) -> Result<TickOutcome, TickError> {
    let config_path = default_config_path(workspace_root);
    if !config_path.exists() {
        // No config → nothing to do. Don't take the registry slot so the
        // next tick re-checks cheaply.
        return Ok(TickOutcome {
            workspace_id,
            ran: false,
            monitors_scanned: 0,
            monitors_failed: 0,
            anomalies_persisted: 0,
        });
    }
    if !registry.take_if_due(workspace_id, min_interval).await {
        return Ok(TickOutcome {
            workspace_id,
            ran: false,
            monitors_scanned: 0,
            monitors_failed: 0,
            anomalies_persisted: 0,
        });
    }
    let scan = scan_workspace(runner, &config_path, Utc::now(), None).await?;
    let persisted = upsert_anomalies(db, workspace_id, &scan).await?;
    Ok(TickOutcome {
        workspace_id,
        ran: true,
        monitors_scanned: scan.outcomes.len(),
        monitors_failed: scan.failures.len(),
        anomalies_persisted: persisted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn registry_lets_first_call_through_and_defers_repeats() {
        let reg = LastScanRegistry::new();
        let ws = Uuid::new_v4();
        let interval = StdDuration::from_secs(60);
        assert!(reg.take_if_due(ws, interval).await);
        assert!(!reg.take_if_due(ws, interval).await);
    }

    #[tokio::test]
    async fn registry_forget_resets_the_lease() {
        let reg = LastScanRegistry::new();
        let ws = Uuid::new_v4();
        let interval = StdDuration::from_secs(60);
        reg.take_if_due(ws, interval).await;
        reg.forget(ws).await;
        assert!(reg.take_if_due(ws, interval).await);
    }
}
