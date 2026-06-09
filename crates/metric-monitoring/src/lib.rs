//! Time-series anomaly detection over the semantic-layer metric tree.
//!
//! Three pieces:
//! - [`config`] — `.monitor.yml` parser; per-workspace list of monitored
//!   (measure, time-dim, granularity) tuples.
//! - [`detect`] — MSTL + AutoETS detector that flags anomalies in a tail
//!   window of a single time-series.
//! - [`service`] — orchestrator that loads the config, fetches each
//!   series via a [`MetricTreeRunner`], runs the detector, and returns
//!   a flat list of [`detect::DetectedAnomaly`] paired with the monitor
//!   that produced them. Persistence lives in the host (`oxy-app`), so the
//!   service doesn't take a database handle.
//!
//! [`MetricTreeRunner`]: agentic_analytics::MetricTreeRunner

pub mod config;
pub mod detect;
pub mod persist;
pub mod service;
pub mod store;
pub mod tick;

pub use config::{
    Direction, Granularity, LoadError as ConfigLoadError, MonitorConfig, MonitorEntry,
    MonitorScheduleConfig, Sensitivity, default_config_path, load_from_file,
};
pub use detect::{DetectError, DetectInputs, DetectedAnomaly, Observation, Severity, detect};
pub use persist::upsert_anomalies;
pub use service::{MonitorOutcome, ScanError, ScanResult, scan_workspace};
pub use tick::{LastScanRegistry, TickError, TickOutcome, global_registry, tick_workspace};
