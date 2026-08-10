//! Anomalies surfaced by `oxy-metric-monitoring`.
//!
//! One row per (workspace, measure, time-dim, period_start). Repeat scans
//! upsert via the unique index so an unresolved anomaly stays visible
//! without piling up duplicates. Status drives the inbox UX:
//! `new` (visible) → `acknowledged` (still visible, marked seen) →
//! `dismissed` (hidden).

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "metric_anomalies")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub measure: String,
    pub time_dimension: String,
    pub granularity: String,
    pub period_start: DateTimeWithTimeZone,
    pub period_end: DateTimeWithTimeZone,
    pub observed: f64,
    pub expected: f64,
    pub lower_bound: f64,
    pub upper_bound: f64,
    pub z_score: f64,
    pub severity: String,
    pub status: String,
    pub label: Option<String>,
    /// Stable key derived from the monitor's filters (e.g. `"sales_daily.restaurant_id=loc-abc"`).
    /// Empty string for chain-wide (unfiltered) monitors. Included in the
    /// unique index so per-store and chain-wide anomalies don't collide.
    pub dimension_key: String,
    /// The raw filters used when fetching the series, stored as JSON so the
    /// API can expose which store/segment this anomaly belongs to.
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub filters: Option<serde_json::Value>,
    /// Cached `airlayer::engine::metric_tree_ops::ExplainResult` for the
    /// drawer. `None` = never computed (or invalidated). Populated lazily
    /// by `POST /semantic/anomalies/{id}/explain`.
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub explain_cache: Option<serde_json::Value>,
    /// Wall-clock when [`Self::explain_cache`] was written. Lets callers
    /// reason about freshness without inspecting the cached payload.
    pub explain_cached_at: Option<DateTimeWithTimeZone>,
    /// Groups consecutive flagged buckets of one segment into a single event,
    /// so a labour surge spanning Mon/Wed/Thu reads as one problem rather than
    /// three. Assigned at persist time by proximity + direction; `None` for
    /// rows detected before events existed.
    ///
    /// Rows stay per-bucket on purpose — `explain_anomaly` reasons about a
    /// single bucket against the same phase one cycle back, so merging rows
    /// would make it describe only the first day of a range.
    pub event_id: Option<Uuid>,
    /// Groups anomalies that fired across *different* segments on the same
    /// bucket in the same direction, so a chain-wide collapse reads as one
    /// event rather than one row per store. Orthogonal to [`Self::event_id`],
    /// which chains consecutive buckets within a single segment — a multi-day
    /// chain-wide slide is both, which is why this is a separate column.
    ///
    /// Membership is recomputed from the scan that observed it rather than
    /// preserved (it is a property of that scan's share, not a historical fact),
    /// but the *identity* is deterministic: derived via `Uuid::new_v5` over the
    /// cohort key, so a restatement re-scan of the same bucket keeps the same id
    /// instead of minting a fresh one each day.
    pub cohort_id: Option<Uuid>,
    /// This member's ratio-to-expectation divided by the cohort's median ratio.
    /// 1.0 is a typical member. Direction-relative: for a *drop* cohort the
    /// actionable rows are **below** 1.0 (fell further than the shared event
    /// explains); for an *increase* cohort the outlier is the one **above** 1.0.
    /// A consumer ranking members must read the cohort's direction to know which
    /// tail to sort toward. `None` outside a cohort, or where no finite ratio
    /// exists.
    pub cohort_deviation: Option<f64>,
    /// The tenant calendar's name for the day this cohort landed on, e.g.
    /// `"Independence Day"`. A label the inbox shows, never a filter that
    /// suppresses the row — see the `m20260804_000001` migration.
    ///
    /// Denormalised at scan time rather than resolved on read: the calendar is
    /// editable config, and this should keep saying what the scan believed.
    pub cohort_label: Option<String>,
    /// Dominant seasonal cycle length (in units of [`Self::granularity`]) from
    /// the monitor's detection config, snapshotted at scan time. Drives the
    /// same-phase comparison window when explaining this anomaly. `None` for
    /// rows detected before this column existed.
    pub seasonal_period: Option<i32>,
    pub detected_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
