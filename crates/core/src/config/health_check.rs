//! Per-workspace health-check cadence, configured in `config.yml` under
//! `health_check:`. The cadence drives the workspace's `health_eval` schedule
//! row (see `agentic_pipeline::scheduler::reconcile_health_schedule`); the eval
//! itself runs as a `TaskSpec::Custom { kind: "health_eval_workspace" }` on the
//! global-run fleet. See
//! `internal-docs/2026-06-26-workspace-scoped-health-checks-design.md`.

use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Lower bound (10m) — the dispatcher tick's effective floor; checking more
/// often than this can't fire sooner anyway.
const MIN_INTERVAL_SECS: u64 = 600;
/// Upper bound (24h).
const MAX_INTERVAL_SECS: u64 = 86_400;
/// Default cadence when `health_check` (or its `interval`) is absent.
const DEFAULT_INTERVAL_SECS: u64 = 600;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HealthCheckConfig {
    /// Evaluation cadence as a humantime duration (e.g. `"30m"`, `"2h"`).
    /// Absent → 10m. Out-of-range or unparseable values clamp/fall back to the
    /// safe default rather than erroring — a malformed cadence must never wedge
    /// the scheduler.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval: Option<String>,
    /// When false, the workspace's health schedule row is disabled and no eval
    /// is enqueued. Defaults to true.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            interval: None,
            enabled: true,
        }
    }
}

/// Resolve the configured cadence to a clamped `Duration` in `[10m, 24h]`.
/// `None`, an absent `interval`, an unparseable value, or an out-of-range value
/// all resolve to a safe in-range duration (default 10m, or the nearest bound).
pub fn resolve_interval(cfg: Option<&HealthCheckConfig>) -> Duration {
    let secs = cfg
        .and_then(|c| c.interval.as_deref())
        .and_then(|s| humantime::parse_duration(s).ok())
        .map(|d| d.as_secs())
        .unwrap_or(DEFAULT_INTERVAL_SECS)
        .clamp(MIN_INTERVAL_SECS, MAX_INTERVAL_SECS);
    Duration::from_secs(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(interval: Option<&str>) -> HealthCheckConfig {
        HealthCheckConfig {
            interval: interval.map(String::from),
            enabled: true,
        }
    }

    #[test]
    fn absent_config_defaults_to_10m() {
        assert_eq!(resolve_interval(None), Duration::from_secs(600));
    }

    #[test]
    fn absent_interval_defaults_to_10m() {
        assert_eq!(resolve_interval(Some(&cfg(None))), Duration::from_secs(600));
    }

    #[test]
    fn parses_humantime() {
        assert_eq!(
            resolve_interval(Some(&cfg(Some("30m")))),
            Duration::from_secs(1800)
        );
        assert_eq!(
            resolve_interval(Some(&cfg(Some("2h")))),
            Duration::from_secs(7200)
        );
    }

    #[test]
    fn clamps_below_floor_to_10m() {
        assert_eq!(
            resolve_interval(Some(&cfg(Some("1m")))),
            Duration::from_secs(600)
        );
    }

    #[test]
    fn clamps_above_ceiling_to_24h() {
        assert_eq!(
            resolve_interval(Some(&cfg(Some("72h")))),
            Duration::from_secs(86_400)
        );
    }

    #[test]
    fn unparseable_falls_back_to_10m() {
        assert_eq!(
            resolve_interval(Some(&cfg(Some("banana")))),
            Duration::from_secs(600)
        );
    }
}
