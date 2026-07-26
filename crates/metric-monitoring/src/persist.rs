//! Persist [`crate::ScanResult`] outcomes to the `metric_anomalies` table.
//!
//! Lives here (not in `oxy-app`) so both the HTTP handler and the cron tick
//! can call into the same upsert path. The unique index on
//! (workspace_id, measure, time_dimension, period_start) means repeat scans
//! update the existing row in place — dismissed rows stay dismissed, others
//! flip back to `new` so a recurring anomaly resurfaces.

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use entity::metric_anomalies::{self, Entity as AnomaliesEntity};
use sea_orm::ActiveValue::Set;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, IntoActiveModel,
    QueryFilter,
};
use uuid::Uuid;

use crate::MonitorEntry;
use crate::config::Granularity;
use crate::config::MonitorFilter;
use crate::detect::{DetectedAnomaly, Severity};
use crate::service::ScanResult;

/// Upsert every flagged anomaly from a scan into the database. Returns the
/// count of rows touched (inserted or updated). Failures are surfaced via
/// the first `DbErr`; callers should log + retry on transient errors.
pub async fn upsert_anomalies(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    scan: &ScanResult,
) -> Result<usize, DbErr> {
    let mut touched = 0usize;
    for outcome in &scan.outcomes {
        for anomaly in &outcome.anomalies {
            upsert_one(db, workspace_id, &outcome.entry, anomaly).await?;
            touched += 1;
        }
    }
    Ok(touched)
}

async fn upsert_one(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    entry: &MonitorEntry,
    anomaly: &DetectedAnomaly,
) -> Result<(), DbErr> {
    let now = Utc::now();
    let period_start = anomaly.timestamp;
    let period_end = period_end_for(period_start, entry.granularity);

    let dim_key = MonitorFilter::key_for(&entry.filters);
    let filters_json = if entry.filters.is_empty() {
        None
    } else {
        serde_json::to_value(&entry.filters).ok()
    };

    // Dominant (nearest) seasonal cycle from the monitor config. Snapshotted so
    // the explain path can align its comparison window to the same phase one
    // cycle back without re-reading the workspace config at request time.
    let seasonal_period = entry
        .effective_seasonality()
        .into_iter()
        .min()
        .map(|p| p as i32);

    let existing = AnomaliesEntity::find()
        .filter(metric_anomalies::Column::WorkspaceId.eq(workspace_id))
        .filter(metric_anomalies::Column::Measure.eq(entry.measure.clone()))
        .filter(metric_anomalies::Column::TimeDimension.eq(entry.time_dimension.clone()))
        .filter(metric_anomalies::Column::DimensionKey.eq(dim_key.clone()))
        .filter(
            metric_anomalies::Column::PeriodStart
                .eq::<chrono::DateTime<chrono::FixedOffset>>(period_start.into()),
        )
        .one(db)
        .await?;

    let severity = severity_to_str(anomaly.severity).to_string();

    if let Some(found) = existing {
        let preserved_status = if found.status == "dismissed" {
            "dismissed".to_string()
        } else {
            // Acknowledged → bumped back to new on a new detection so the
            // user sees the recurrence.
            "new".to_string()
        };
        let mut active = found.into_active_model();
        active.observed = Set(anomaly.observed);
        active.expected = Set(anomaly.expected);
        active.lower_bound = Set(anomaly.lower);
        active.upper_bound = Set(anomaly.upper);
        active.z_score = Set(anomaly.z_score);
        active.severity = Set(severity);
        active.status = Set(preserved_status);
        active.dimension_key = Set(dim_key);
        active.filters = Set(filters_json);
        active.seasonal_period = Set(seasonal_period);
        active.updated_at = Set(now.into());
        active.update(db).await?;
    } else {
        metric_anomalies::ActiveModel {
            id: Set(Uuid::new_v4()),
            workspace_id: Set(workspace_id),
            measure: Set(entry.measure.clone()),
            time_dimension: Set(entry.time_dimension.clone()),
            granularity: Set(entry.granularity.airlayer_str().to_string()),
            period_start: Set(period_start.into()),
            period_end: Set(period_end.into()),
            observed: Set(anomaly.observed),
            expected: Set(anomaly.expected),
            lower_bound: Set(anomaly.lower),
            upper_bound: Set(anomaly.upper),
            z_score: Set(anomaly.z_score),
            severity: Set(severity),
            status: Set("new".to_string()),
            label: Set(entry.label.clone()),
            dimension_key: Set(dim_key),
            filters: Set(filters_json),
            seasonal_period: Set(seasonal_period),
            // Lazy: populated on first /explain call.
            explain_cache: Set(None),
            explain_cached_at: Set(None),
            detected_at: Set(now.into()),
            updated_at: Set(now.into()),
        }
        .insert(db)
        .await?;
    }
    Ok(())
}

fn severity_to_str(s: Severity) -> &'static str {
    match s {
        Severity::Low => "low",
        Severity::Medium => "medium",
        Severity::High => "high",
    }
}

fn period_end_for(start: DateTime<Utc>, granularity: Granularity) -> DateTime<Utc> {
    match granularity {
        Granularity::Day => start + Duration::days(1),
        Granularity::Week => start + Duration::weeks(1),
        Granularity::Month => {
            let d = start.date_naive();
            let next = if d.month() == 12 {
                NaiveDate::from_ymd_opt(d.year() + 1, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1)
            };
            next.unwrap_or(d + Duration::days(31))
                .and_hms_opt(0, 0, 0)
                .and_then(|dt| Utc.from_local_datetime(&dt).single())
                .unwrap_or(start + Duration::days(31))
        }
    }
}
