use std::sync::Arc;

use agentic_analytics::anomaly_store::{
    AnomalyFilter, AnomalyRecord, AnomalyStoreError, DetectAndUpsertResult,
};
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, TimeZone, Utc};
use entity::metric_anomalies::{self, Entity as AnomaliesEntity};
use sea_orm::ActiveValue::Set;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder,
};
use uuid::Uuid;

use crate::config::Sensitivity;
use crate::detect::{DetectInputs, Observation, Severity, detect};

pub struct OxyAnomalyStore {
    pub db: Arc<DatabaseConnection>,
}

fn parse_utc(s: &str) -> Option<DateTime<Utc>> {
    s.parse::<DateTime<FixedOffset>>()
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

fn seasonal_periods(granularity: &str) -> Vec<usize> {
    match granularity {
        "day" => vec![7],
        "week" => vec![4],
        "month" => vec![12],
        "quarter" => vec![4],
        _ => vec![7],
    }
}

fn granularity_period_end(start: DateTime<Utc>, granularity: &str) -> DateTime<Utc> {
    match granularity {
        "week" => start + Duration::weeks(1),
        "month" => next_month_start(start.date_naive())
            .and_hms_opt(0, 0, 0)
            .and_then(|dt| Utc.from_local_datetime(&dt).single())
            .unwrap_or(start + Duration::days(31)),
        "quarter" => next_quarter_start(start.date_naive())
            .and_hms_opt(0, 0, 0)
            .and_then(|dt| Utc.from_local_datetime(&dt).single())
            .unwrap_or(start + Duration::days(91)),
        _ => start + Duration::days(1),
    }
}

fn next_month_start(d: NaiveDate) -> NaiveDate {
    if d.month() == 12 {
        NaiveDate::from_ymd_opt(d.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1)
    }
    .unwrap_or(d + Duration::days(31))
}

fn next_quarter_start(d: NaiveDate) -> NaiveDate {
    let next_quarter_month = ((d.month() - 1) / 3 + 1) * 3 + 1;
    let (y, m) = if next_quarter_month > 12 {
        (d.year() + 1, next_quarter_month - 12)
    } else {
        (d.year(), next_quarter_month)
    };
    NaiveDate::from_ymd_opt(y, m, 1).unwrap_or(d + Duration::days(91))
}

fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Low => "low",
        Severity::Medium => "medium",
        Severity::High => "high",
    }
}

fn to_record(m: &metric_anomalies::Model) -> AnomalyRecord {
    AnomalyRecord {
        id: m.id,
        measure: m.measure.clone(),
        time_dimension: m.time_dimension.clone(),
        granularity: m.granularity.clone(),
        period_start: m.period_start.to_rfc3339(),
        period_end: m.period_end.to_rfc3339(),
        observed: m.observed,
        expected: m.expected,
        lower: m.lower_bound,
        upper: m.upper_bound,
        z_score: m.z_score,
        severity: m.severity.clone(),
        status: m.status.clone(),
    }
}

#[async_trait::async_trait]
impl agentic_analytics::anomaly_store::AnomalyStore for OxyAnomalyStore {
    async fn list(
        &self,
        workspace_id: Uuid,
        filter: AnomalyFilter,
    ) -> Result<Vec<AnomalyRecord>, AnomalyStoreError> {
        let mut q = AnomaliesEntity::find()
            .filter(metric_anomalies::Column::WorkspaceId.eq(workspace_id))
            .order_by_desc(metric_anomalies::Column::ZScore);

        if let Some(m) = filter.measure {
            q = q.filter(metric_anomalies::Column::Measure.eq(m));
        }
        if let Some(td) = filter.time_dimension {
            q = q.filter(metric_anomalies::Column::TimeDimension.eq(td));
        }
        if let Some(g) = filter.granularity {
            q = q.filter(metric_anomalies::Column::Granularity.eq(g));
        }
        if let Some(ps) = filter.period_start_gte {
            if let Some(dt) = parse_utc(&ps) {
                let fdt: DateTime<FixedOffset> = dt.into();
                q = q.filter(metric_anomalies::Column::PeriodStart.gte(fdt));
            }
        }
        if let Some(pe) = filter.period_end_lte {
            if let Some(dt) = parse_utc(&pe) {
                let fdt: DateTime<FixedOffset> = dt.into();
                q = q.filter(metric_anomalies::Column::PeriodEnd.lte(fdt));
            }
        }

        let rows = q
            .all(self.db.as_ref())
            .await
            .map_err(|e| AnomalyStoreError::Db(e.to_string()))?;
        Ok(rows.iter().map(to_record).collect())
    }

    async fn get(
        &self,
        id: Uuid,
        workspace_id: Uuid,
    ) -> Result<Option<AnomalyRecord>, AnomalyStoreError> {
        let row = AnomaliesEntity::find_by_id(id)
            .filter(metric_anomalies::Column::WorkspaceId.eq(workspace_id))
            .one(self.db.as_ref())
            .await
            .map_err(|e| AnomalyStoreError::Db(e.to_string()))?;
        Ok(row.as_ref().map(to_record))
    }

    async fn detect_and_upsert(
        &self,
        workspace_id: Uuid,
        measure: &str,
        time_dimension: &str,
        granularity: &str,
        observations: Vec<(String, f64)>,
    ) -> Result<DetectAndUpsertResult, AnomalyStoreError> {
        let series: Vec<Observation> = observations
            .iter()
            .filter_map(|(ts, val)| {
                parse_utc(ts).map(|timestamp| Observation {
                    timestamp,
                    value: *val,
                })
            })
            .collect();

        let seasonal = seasonal_periods(granularity);
        let max_period = seasonal.iter().copied().max().unwrap_or(7);
        // Mirror detect()'s formula exactly: (max_period*2).max(10) + test_window.
        // test_window is hardcoded to 1 below; keeping it consistent prevents
        // detect() from returning SeriesTooShort on a series that passed this guard.
        let min_obs = (max_period * 2).max(10) + 1;

        if series.len() < min_obs {
            return Ok(DetectAndUpsertResult {
                anomalies: vec![],
                total_observations: series.len(),
                message: Some(format!(
                    "Not enough data: {} observations, need at least {}",
                    series.len(),
                    min_obs
                )),
            });
        }

        let inputs = DetectInputs {
            series: &series,
            seasonal_periods: seasonal,
            test_window: 1,
            sensitivity: Sensitivity::Medium,
            interval_level: 0.95,
        };
        let detected = detect(inputs).map_err(|e| AnomalyStoreError::Detection(e.to_string()))?;
        let total = series.len();
        let now = Utc::now();
        let mut records = Vec::with_capacity(detected.len());

        for anomaly in &detected {
            let period_start: DateTime<FixedOffset> = anomaly.timestamp.into();
            let period_end: DateTime<FixedOffset> =
                granularity_period_end(anomaly.timestamp, granularity).into();

            // detect_and_upsert is the AI-tool path — it has no segment context,
            // so it always stores dimension_key = "". Scope the lookup to that
            // value to avoid matching periodic-scan rows for specific segments.
            let existing = AnomaliesEntity::find()
                .filter(metric_anomalies::Column::WorkspaceId.eq(workspace_id))
                .filter(metric_anomalies::Column::Measure.eq(measure))
                .filter(metric_anomalies::Column::TimeDimension.eq(time_dimension))
                .filter(metric_anomalies::Column::DimensionKey.eq(""))
                .filter(metric_anomalies::Column::PeriodStart.eq(period_start))
                .one(self.db.as_ref())
                .await
                .map_err(|e| AnomalyStoreError::Db(e.to_string()))?;

            let model = if let Some(found) = existing {
                let preserved = if found.status == "dismissed" {
                    "dismissed"
                } else {
                    "new"
                };
                let mut a = found.into_active_model();
                a.observed = Set(anomaly.observed);
                a.expected = Set(anomaly.expected);
                a.lower_bound = Set(anomaly.lower);
                a.upper_bound = Set(anomaly.upper);
                a.z_score = Set(anomaly.z_score);
                a.severity = Set(severity_str(anomaly.severity).to_string());
                a.status = Set(preserved.to_string());
                a.updated_at = Set(now.into());
                a.update(self.db.as_ref())
                    .await
                    .map_err(|e| AnomalyStoreError::Db(e.to_string()))?
            } else {
                metric_anomalies::ActiveModel {
                    id: Set(Uuid::new_v4()),
                    workspace_id: Set(workspace_id),
                    measure: Set(measure.to_string()),
                    time_dimension: Set(time_dimension.to_string()),
                    granularity: Set(granularity.to_string()),
                    period_start: Set(period_start),
                    period_end: Set(period_end),
                    observed: Set(anomaly.observed),
                    expected: Set(anomaly.expected),
                    lower_bound: Set(anomaly.lower),
                    upper_bound: Set(anomaly.upper),
                    z_score: Set(anomaly.z_score),
                    severity: Set(severity_str(anomaly.severity).to_string()),
                    status: Set("new".to_string()),
                    label: Set(None),
                    dimension_key: Set(String::new()),
                    filters: Set(None),
                    explain_cache: Set(None),
                    explain_cached_at: Set(None),
                    detected_at: Set(now.into()),
                    updated_at: Set(now.into()),
                }
                .insert(self.db.as_ref())
                .await
                .map_err(|e| AnomalyStoreError::Db(e.to_string()))?
            };

            records.push(to_record(&model));
        }

        Ok(DetectAndUpsertResult {
            anomalies: records,
            total_observations: total,
            message: None,
        })
    }

    async fn get_explain_cache(
        &self,
        id: Uuid,
        workspace_id: Uuid,
    ) -> Result<Option<serde_json::Value>, AnomalyStoreError> {
        let row = AnomaliesEntity::find_by_id(id)
            .filter(metric_anomalies::Column::WorkspaceId.eq(workspace_id))
            .one(self.db.as_ref())
            .await
            .map_err(|e| AnomalyStoreError::Db(e.to_string()))?;
        Ok(row.and_then(|r| r.explain_cache))
    }

    async fn set_explain_cache(
        &self,
        id: Uuid,
        workspace_id: Uuid,
        result: serde_json::Value,
    ) -> Result<(), AnomalyStoreError> {
        let now = Utc::now();
        let row = AnomaliesEntity::find_by_id(id)
            .filter(metric_anomalies::Column::WorkspaceId.eq(workspace_id))
            .one(self.db.as_ref())
            .await
            .map_err(|e| AnomalyStoreError::Db(e.to_string()))?
            .ok_or_else(|| AnomalyStoreError::NotFound(id.to_string()))?;

        let mut a = row.into_active_model();
        a.explain_cache = Set(Some(result));
        a.explain_cached_at = Set(Some(now.into()));
        a.update(self.db.as_ref())
            .await
            .map_err(|e| AnomalyStoreError::Db(e.to_string()))?;
        Ok(())
    }
}
