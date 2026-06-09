//! `.monitor.yml` config files — one per workspace, declaring which
//! measures get anomaly-monitored.
//!
//! A single workspace's monitor config is a list of [`MonitorEntry`], each
//! pointing at a measure + time-dimension + granularity in the semantic layer.
//! Decoupled from airlayer's `Measure` schema so we don't have to land an
//! upstream PR for an experimental field.
//!
//! Example:
//!
//! ```yaml
//! # workspaces/.../.monitor.yml
//! monitors:
//!   - measure: orders.revenue
//!     time_dimension: orders.created_at
//!     granularity: day
//!     lookback_days: 90
//!     seasonality: [7, 30]
//!     sensitivity: medium
//!
//!   - measure: orders.refund_rate
//!     time_dimension: orders.created_at
//!     granularity: week
//!     lookback_days: 365
//!     seasonality: [4]   # ~monthly cycle in week buckets
//! ```

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Per-granularity cron expressions read from `.monitor.yml`.
/// All fields are optional — only declared granularities get schedule rows.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MonitorScheduleConfig {
    /// Cron expression for day-granularity monitors (e.g. `"0 6 * * *"`).
    pub daily: Option<String>,
    /// Cron expression for week-granularity monitors (e.g. `"0 6 * * 1"`).
    pub weekly: Option<String>,
    /// Cron expression for month-granularity monitors (e.g. `"5 6 1 * *"`).
    pub monthly: Option<String>,
}

/// All monitor entries declared for a workspace.
///
/// Loaded from a single `.monitor.yml` file under the workspace's semantics
/// scan path. Empty file / missing file → empty config (monitoring is opt-in).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MonitorConfig {
    /// Per-granularity cron schedule. Absent = no automated scanning.
    #[serde(default)]
    pub schedule: Option<MonitorScheduleConfig>,
    #[serde(default)]
    pub monitors: Vec<MonitorEntry>,
}

/// A dimension filter applied when fetching the monitor's time series.
/// Corresponds to an airlayer `equals` filter on a single member.
///
/// Example (YAML):
/// ```yaml
/// filters:
///   - member: sales_daily.restaurant_id
///     values: ["loc-abc123"]
/// ```
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct MonitorFilter {
    /// Fully-qualified dimension id, e.g. `"sales_daily.restaurant_id"`.
    pub member: String,
    /// Values to match (OR within a single filter, AND across filters).
    pub values: Vec<String>,
}

impl MonitorFilter {
    /// Stable string key suitable for use in a unique index.
    /// Format: `member=v1,v2;member2=v3` (members sorted).
    pub fn key_for(filters: &[MonitorFilter]) -> String {
        if filters.is_empty() {
            return String::new();
        }
        let mut pairs: Vec<String> = filters
            .iter()
            .map(|f| {
                let mut vals = f.values.clone();
                vals.sort();
                format!("{}={}", f.member, vals.join(","))
            })
            .collect();
        pairs.sort();
        pairs.join(";")
    }
}

/// A single (measure, time-dim, granularity) tuple to monitor.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MonitorEntry {
    /// Fully qualified measure id, e.g. `"orders.revenue"`. Must exist in
    /// the workspace's semantic layer at scan time.
    pub measure: String,
    /// Fully qualified time-dimension id used to bucket the series.
    pub time_dimension: String,
    /// Aggregation grain — `day`, `week`, `month`. The detection lookback
    /// is sampled at this grain.
    #[serde(default = "default_granularity")]
    pub granularity: Granularity,
    /// How many days of history to fit the forecast against. Default 90 for
    /// daily, but the loader bumps this implicitly when granularity is
    /// coarser so MSTL has enough cycles.
    #[serde(default = "default_lookback_days")]
    pub lookback_days: u32,
    /// Seasonal periods (in units of `granularity`) to decompose against.
    /// Defaults to `[7]` (weekly) for daily series; `[4]` for weekly;
    /// `[12]` for monthly. Override to add daily-of-month / yearly etc.
    #[serde(default)]
    pub seasonality: Option<Vec<usize>>,
    /// Detection threshold — controls how many σ the residual must exceed
    /// the forecast band before we flag it. Maps to a z-score cutoff
    /// inside the detector.
    #[serde(default)]
    pub sensitivity: Sensitivity,
    /// Optional human label used in the inbox UI. Falls back to the
    /// measure id when absent.
    #[serde(default)]
    pub label: Option<String>,
    /// Optional dimension filters applied when fetching the series.
    /// Use this to monitor a single store / segment rather than the
    /// chain-wide aggregate.
    #[serde(default)]
    pub filters: Vec<MonitorFilter>,
    /// Fan-out dimension: instead of writing one entry per segment value,
    /// declare the dimension here and the scanner discovers all active
    /// values at scan time and runs one detector per segment automatically.
    ///
    /// Example:
    /// ```yaml
    /// group_by: sales_daily.restaurant_id
    /// ```
    ///
    /// `group_by` is applied in addition to any `filters` already declared
    /// (AND semantics). For example, `filters: [{region: US}]` combined with
    /// `group_by: restaurant_id` monitors each US restaurant individually.
    #[serde(default)]
    pub group_by: Option<String>,
    /// Which direction of deviation to flag. Defaults to `both`.
    /// Use `decrease` for metrics where only drops matter (revenue, order
    /// volume, pipeline row counts) and `increase` for metrics where only
    /// spikes matter (error rates, latency, costs).
    #[serde(default)]
    pub direction: Direction,
}

/// Which direction of anomaly to surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Flag both unexpected drops and unexpected spikes (default).
    #[default]
    Both,
    /// Flag only when the observed value is below the expected (drops).
    Decrease,
    /// Flag only when the observed value is above the expected (spikes).
    Increase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Granularity {
    Day,
    Week,
    Month,
}

impl Granularity {
    /// String matching airlayer's `TimeDimensionQuery::granularity` field.
    pub fn airlayer_str(self) -> &'static str {
        match self {
            Granularity::Day => "day",
            Granularity::Week => "week",
            Granularity::Month => "month",
        }
    }

    /// Default seasonal periods when the user doesn't override.
    /// Single-period defaults — daily gets weekly, weekly gets monthly-ish,
    /// monthly gets yearly. Multi-seasonality (e.g. day + week + year) is
    /// available by overriding `seasonality` in the YAML.
    pub fn default_seasonality(self) -> Vec<usize> {
        match self {
            Granularity::Day => vec![7],
            Granularity::Week => vec![4],
            Granularity::Month => vec![12],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Sensitivity {
    /// Flag only large deviations (~5σ). Fewer false positives, may miss
    /// subtle moves.
    Low,
    /// Default. Flag at ~3σ.
    #[default]
    Medium,
    /// Flag at ~2σ. Noisier; useful for dashboards under active investigation.
    High,
}

impl Sensitivity {
    /// Residual z-score cutoff. Detector flags when `|residual| > σ * cutoff`.
    pub fn z_cutoff(self) -> f64 {
        match self {
            Sensitivity::Low => 5.0,
            Sensitivity::Medium => 3.0,
            Sensitivity::High => 2.0,
        }
    }
}

fn default_granularity() -> Granularity {
    Granularity::Day
}

fn default_lookback_days() -> u32 {
    90
}

/// Failure modes when loading a monitor config.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("monitor config not found at {0}")]
    NotFound(PathBuf),
    #[error("failed to read monitor config at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid monitor config at {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },
}

/// Load a single `.monitor.yml` from disk. Missing file returns an empty
/// config (monitoring is opt-in; the absence of a config is not an error).
pub fn load_from_file(path: &Path) -> Result<MonitorConfig, LoadError> {
    if !path.exists() {
        return Ok(MonitorConfig::default());
    }
    let text = std::fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if text.trim().is_empty() {
        return Ok(MonitorConfig::default());
    }
    serde_yaml::from_str(&text).map_err(|source| LoadError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

/// Convenience: the conventional location of the config inside a workspace
/// (`<workspace>/.monitor.yml`). Callers may also accept user-provided paths.
pub fn default_config_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".monitor.yml")
}

impl MonitorEntry {
    /// Resolved seasonal periods — explicit override wins over the
    /// granularity default.
    pub fn effective_seasonality(&self) -> Vec<usize> {
        self.seasonality
            .clone()
            .unwrap_or_else(|| self.granularity.default_seasonality())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_entry() {
        let yaml = r#"
monitors:
  - measure: orders.revenue
    time_dimension: orders.created_at
"#;
        let cfg: MonitorConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.monitors.len(), 1);
        let m = &cfg.monitors[0];
        assert_eq!(m.measure, "orders.revenue");
        assert_eq!(m.granularity, Granularity::Day);
        assert_eq!(m.lookback_days, 90);
        assert_eq!(m.sensitivity, Sensitivity::Medium);
        assert_eq!(m.effective_seasonality(), vec![7]);
    }

    #[test]
    fn rejects_unknown_fields() {
        let yaml = r#"
monitors:
  - measure: orders.revenue
    time_dimension: orders.created_at
    unknown_field: 42
"#;
        assert!(serde_yaml::from_str::<MonitorConfig>(yaml).is_err());
    }

    #[test]
    fn sensitivity_z_cutoffs() {
        assert!(Sensitivity::Low.z_cutoff() > Sensitivity::Medium.z_cutoff());
        assert!(Sensitivity::Medium.z_cutoff() > Sensitivity::High.z_cutoff());
    }

    #[test]
    fn parses_schedule_block() {
        let yaml = r#"
schedule:
  daily: "0 6 * * *"
  weekly: "0 6 * * 1"

monitors:
  - measure: orders.revenue
    time_dimension: orders.created_at
"#;
        let cfg: MonitorConfig = serde_yaml::from_str(yaml).unwrap();
        let sched = cfg.schedule.unwrap();
        assert_eq!(sched.daily.as_deref(), Some("0 6 * * *"));
        assert_eq!(sched.weekly.as_deref(), Some("0 6 * * 1"));
        assert!(sched.monthly.is_none());
    }

    #[test]
    fn schedule_block_optional() {
        let yaml = r#"
monitors:
  - measure: orders.revenue
    time_dimension: orders.created_at
"#;
        let cfg: MonitorConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cfg.schedule.is_none());
    }
}
