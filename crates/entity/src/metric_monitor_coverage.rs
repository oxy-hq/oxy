//! How much history each monitored segment actually has.
//!
//! One row per (workspace, measure, time-dim, dimension_key), upserted on
//! every scan. Exists so a segment that is being **skipped** for want of
//! history can say so: `scan_one` returns `Ok(vec![])` there and deliberately
//! stays out of `monitors_failed`, which otherwise leaves an empty inbox
//! looking identical to a healthy one.
//!
//! Scoring happens exactly when `measured_buckets >= required_buckets`.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "metric_monitor_coverage")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub measure: String,
    pub time_dimension: String,
    pub granularity: String,
    /// Stable key derived from the monitor's filters, matching
    /// `metric_anomalies::Model::dimension_key`. Empty for chain-wide monitors.
    pub dimension_key: String,
    /// The filters this segment was fetched with, so the UI can name it.
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub filters: Option<serde_json::Value>,
    pub label: Option<String>,
    /// Buckets the warehouse actually returned — gaps that `fill_gaps`
    /// invented are **not** counted, matching the guard in `scan_one`.
    pub measured_buckets: i32,
    /// The statistical floor from `gates::min_history_buckets` at scan time.
    pub required_buckets: i32,
    pub last_scanned_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    /// Whether this segment is accumulating history rather than being scored.
    pub fn is_warming_up(&self) -> bool {
        self.measured_buckets < self.required_buckets
    }
}
