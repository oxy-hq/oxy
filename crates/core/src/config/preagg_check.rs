//! Per-workspace pre-aggregation cadence, resolved from `config.yml`'s
//! `pre_aggregations:` block. Drives the workspace's `preagg_cycle` schedule
//! row (see `agentic_pipeline::scheduler::reconcile_preagg_schedule`); the
//! cycle itself runs as a `TaskSpec::Custom { kind: "preagg_cycle" }` on the
//! global-run fleet — mirrors `health_check.rs` / `health_eval_workspace`
//! exactly, once removed: build a rollup, not evaluate one.
//!
//! **Off until a workspace writes the block.** No `pre_aggregations:` block
//! means no rollup was ever declared with intent, so no cycle is scheduled —
//! same "the field default covers a written block; resolve_enabled covers no
//! block at all" split as [`crate::config::health_check::resolve_enabled`].
//! This is a behavior change from the pre-scheduling worker, which ran
//! unconditionally off the server's own startup directory and simply found
//! nothing to build for a workspace with no block; enumerating every
//! workspace in a fleet on a fixed cadence makes that skip worth avoiding.

use std::time::Duration;

use super::model::PreaggConfig;

/// Lower bound (30s) — matches the in-process global driver's own tick
/// granularity (`OXY_INPROC_GLOBAL_WORKER`'s default `interval_secs`), so
/// nothing configured finer than that could fire sooner anyway.
const MIN_INTERVAL_SECS: u64 = 30;
/// Upper bound (24h), matching the health-check ceiling.
const MAX_INTERVAL_SECS: u64 = 86_400;
/// Default cadence when `pre_aggregations.refresh_worker.heartbeat` is absent.
/// Matches what `examples/config.yml` has always written explicitly; the
/// pre-scheduling worker's hardcoded 30s default was tuned for a single local
/// workspace; it stops making sense once every tenant is scheduled.
const DEFAULT_INTERVAL_SECS: u64 = 600;

/// Renewal-threshold bounds — how long a cached `sql:` refresh-key result is
/// trusted before re-evaluating. Unrelated to schedule cadence: this gates the
/// Layer-1 per-query cache, not how often the Layer-2 cycle itself fires.
const MIN_RENEWAL_SECS: u64 = 10;
const MAX_RENEWAL_SECS: u64 = 3_600;
/// Public so the request path's own fallback is THIS number rather than a
/// second hardcoded 120 that can drift from it.
pub const DEFAULT_RENEWAL_SECS: u64 = 120;

const DEFAULT_SCHEMA: &str = "AIRLAYER";

/// Whether a workspace's pre-aggregations are scheduled at all. `None` (no
/// block) and an explicit `refresh_worker.enabled: false` both mean off; a
/// written block with `refresh_worker` omitted, or `enabled` omitted within
/// it, means on — the block's presence is the opt-in, matching
/// [`crate::config::health_check::resolve_enabled`]'s split for the same
/// reason: a field default can't distinguish "chose the default" from "never
/// asked".
pub fn resolve_enabled(cfg: Option<&PreaggConfig>) -> bool {
    cfg.is_some_and(|c| {
        c.refresh_worker
            .as_ref()
            .and_then(|w| w.enabled)
            .unwrap_or(true)
    })
}

/// Resolve the configured build cadence to a clamped `Duration` in
/// `[30s, 24h]`. Absent, unparseable, or out-of-range all resolve to a safe
/// in-range duration (default 10m, or the nearest bound) — a malformed
/// cadence must never wedge the scheduler.
pub fn resolve_interval(cfg: Option<&PreaggConfig>) -> Duration {
    let secs = cfg
        .and_then(|c| c.refresh_worker.as_ref())
        .and_then(|w| w.heartbeat.as_deref())
        .and_then(|s| humantime::parse_duration(s).ok())
        .map(|d| d.as_secs())
        .unwrap_or(DEFAULT_INTERVAL_SECS)
        .clamp(MIN_INTERVAL_SECS, MAX_INTERVAL_SECS);
    Duration::from_secs(secs)
}

/// Resolve the Layer-1 refresh-key renewal threshold to a clamped `Duration`
/// in `[10s, 1h]`. Same fail-safe contract as [`resolve_interval`].
pub fn resolve_renewal_threshold(cfg: Option<&PreaggConfig>) -> Duration {
    let secs = cfg
        .and_then(|c| c.refresh_worker.as_ref())
        .and_then(|w| w.renewal_threshold.as_deref())
        .and_then(|s| humantime::parse_duration(s).ok())
        .map(|d| d.as_secs())
        .unwrap_or(DEFAULT_RENEWAL_SECS)
        .clamp(MIN_RENEWAL_SECS, MAX_RENEWAL_SECS);
    Duration::from_secs(secs)
}

/// Warehouse schema pre-agg tables are created under. Defaults to `"AIRLAYER"`.
pub fn resolve_schema(cfg: Option<&PreaggConfig>) -> String {
    cfg.and_then(|c| c.schema.clone())
        .unwrap_or_else(|| DEFAULT_SCHEMA.to_string())
}

/// Database connector override for all rollup builds. `None` means each view
/// uses its own `datasource`.
pub fn resolve_database(cfg: Option<&PreaggConfig>) -> Option<String> {
    cfg.and_then(|c| c.database.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::RefreshWorkerConfig;

    #[test]
    fn no_block_is_disabled() {
        assert!(!resolve_enabled(None));
    }

    #[test]
    fn a_written_block_with_no_refresh_worker_section_defaults_enabled() {
        let cfg = PreaggConfig::default();
        assert!(resolve_enabled(Some(&cfg)));
    }

    #[test]
    fn explicit_disable_wins() {
        let cfg = PreaggConfig {
            refresh_worker: Some(RefreshWorkerConfig {
                enabled: Some(false),
                heartbeat: None,
                renewal_threshold: None,
            }),
            ..Default::default()
        };
        assert!(!resolve_enabled(Some(&cfg)));
    }

    #[test]
    fn interval_clamps_out_of_range_values() {
        let too_fast = PreaggConfig {
            refresh_worker: Some(RefreshWorkerConfig {
                enabled: None,
                heartbeat: Some("1s".to_string()),
                renewal_threshold: None,
            }),
            ..Default::default()
        };
        assert_eq!(
            resolve_interval(Some(&too_fast)),
            Duration::from_secs(MIN_INTERVAL_SECS)
        );

        let too_slow = PreaggConfig {
            refresh_worker: Some(RefreshWorkerConfig {
                enabled: None,
                heartbeat: Some("30d".to_string()),
                renewal_threshold: None,
            }),
            ..Default::default()
        };
        assert_eq!(
            resolve_interval(Some(&too_slow)),
            Duration::from_secs(MAX_INTERVAL_SECS)
        );
    }

    #[test]
    fn interval_defaults_when_absent_or_unparsable() {
        assert_eq!(
            resolve_interval(None),
            Duration::from_secs(DEFAULT_INTERVAL_SECS)
        );
        let garbage = PreaggConfig {
            refresh_worker: Some(RefreshWorkerConfig {
                enabled: None,
                heartbeat: Some("not a duration".to_string()),
                renewal_threshold: None,
            }),
            ..Default::default()
        };
        assert_eq!(
            resolve_interval(Some(&garbage)),
            Duration::from_secs(DEFAULT_INTERVAL_SECS)
        );
    }

    #[test]
    fn schema_and_database_defaults() {
        assert_eq!(resolve_schema(None), "AIRLAYER");
        assert_eq!(resolve_database(None), None);
        let cfg = PreaggConfig {
            schema: Some("custom".to_string()),
            database: Some("warehouse".to_string()),
            ..Default::default()
        };
        assert_eq!(resolve_schema(Some(&cfg)), "custom");
        assert_eq!(resolve_database(Some(&cfg)), Some("warehouse".to_string()));
    }
}
