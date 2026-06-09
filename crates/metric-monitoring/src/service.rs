//! Top-level orchestrator: load monitor config → fetch each series via the
//! injected [`MetricTreeRunner`] → run [`detect`] → return a flat scan
//! result. Persistence lives in the host (the `oxy-app` handler converts
//! [`MonitorOutcome`]s into SeaORM rows); the service itself is database-free
//! so it tests cleanly with a fake runner.

use std::path::Path;
use std::sync::Arc;

use agentic_analytics::{MetricTreeRunner, MetricTreeRunnerError};
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};

use crate::config::{
    Direction, Granularity, LoadError, MonitorConfig, MonitorEntry, MonitorFilter, load_from_file,
};
use crate::detect::{
    DetectError, DetectInputs, DetectedAnomaly, Observation, detect as run_detect,
};

/// Result of scanning a workspace.
#[derive(Debug, Default)]
pub struct ScanResult {
    /// One entry per monitor that ran successfully (with or without
    /// flagged anomalies).
    pub outcomes: Vec<MonitorOutcome>,
    /// One entry per monitor that errored out — kept separate so a single
    /// broken monitor doesn't fail the whole scan.
    pub failures: Vec<MonitorFailure>,
}

/// What one monitor produced.
#[derive(Debug)]
pub struct MonitorOutcome {
    pub entry: MonitorEntry,
    pub anomalies: Vec<DetectedAnomaly>,
}

#[derive(Debug)]
pub struct MonitorFailure {
    pub entry: MonitorEntry,
    pub error: ScanError,
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("loading monitor config: {0}")]
    LoadConfig(#[from] LoadError),
    #[error("fetching time series: {0}")]
    FetchSeries(#[from] MetricTreeRunnerError),
    #[error("detecting anomalies: {0}")]
    Detect(#[from] DetectError),
    #[error("parsing timestamp '{ts}' from warehouse: {source}")]
    ParseTimestamp {
        ts: String,
        #[source]
        source: chrono::ParseError,
    },
}

/// Scan every monitor in the workspace's `.monitor.yml`.
///
/// `config_path` is usually [`crate::config::default_config_path`] but the
/// admin scan endpoint may accept overrides for one-shot what-if scans.
///
/// `now` is injected so tests are deterministic; production passes
/// `Utc::now()`.
pub async fn scan_workspace(
    runner: Arc<dyn MetricTreeRunner>,
    config_path: &Path,
    now: DateTime<Utc>,
    granularity_filter: Option<Granularity>,
) -> Result<ScanResult, ScanError> {
    let cfg: MonitorConfig = load_from_file(config_path)?;
    let mut result = ScanResult::default();

    // Apply granularity filter before expanding group_by entries.
    let monitors: Vec<MonitorEntry> = if let Some(gran) = granularity_filter {
        cfg.monitors
            .into_iter()
            .filter(|m| m.granularity == gran)
            .collect()
    } else {
        cfg.monitors
    };

    // Expand group_by entries into one entry per discovered dimension value.
    // Entries without group_by pass through unchanged.
    let mut expanded: Vec<MonitorEntry> = Vec::new();
    for entry in monitors {
        if let Some(ref dim) = entry.group_by {
            match runner
                .get_dimension_values(dim.clone(), entry.measure.clone(), entry.lookback_days)
                .await
            {
                Ok(values) => {
                    if values.is_empty() {
                        tracing::warn!(
                            target: "metric_monitoring",
                            measure = %entry.measure,
                            dimension = %dim,
                            "group_by produced no dimension values — skipping"
                        );
                    }
                    for value in values {
                        let mut segment = entry.clone();
                        segment.group_by = None;
                        // AND the group_by filter with any existing filters so
                        // e.g. `filters: [{region: US}]` + `group_by: restaurant_id`
                        // queries each restaurant scoped to US only.
                        segment.filters.push(MonitorFilter {
                            member: dim.clone(),
                            values: vec![value],
                        });
                        expanded.push(segment);
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        target: "metric_monitoring",
                        measure = %entry.measure,
                        dimension = %dim,
                        error = %e,
                        "failed to discover group_by dimension values"
                    );
                    result.failures.push(MonitorFailure {
                        entry,
                        error: ScanError::FetchSeries(e),
                    });
                }
            }
        } else {
            expanded.push(entry);
        }
    }

    // Run monitors concurrently so a slow warehouse query on one monitor does
    // not block the others. Results arrive in completion order via JoinSet.
    let mut set = tokio::task::JoinSet::new();
    for entry in expanded {
        let runner = runner.clone();
        set.spawn(async move {
            let outcome = scan_one(runner, &entry, now).await;
            (entry, outcome)
        });
    }

    while let Some(join_result) = set.join_next().await {
        match join_result {
            Ok((entry, Ok(anomalies))) => result.outcomes.push(MonitorOutcome { entry, anomalies }),
            Ok((entry, Err(error))) => {
                tracing::warn!(
                    target: "metric_monitoring",
                    measure = %entry.measure,
                    error = %error,
                    "monitor scan failed"
                );
                result.failures.push(MonitorFailure { entry, error });
            }
            Err(join_err) => {
                tracing::error!(
                    target: "metric_monitoring",
                    error = %join_err,
                    "monitor task panicked"
                );
            }
        }
    }

    Ok(result)
}

/// Run a single monitor end-to-end.
pub async fn scan_one(
    runner: Arc<dyn MetricTreeRunner>,
    entry: &MonitorEntry,
    now: DateTime<Utc>,
) -> Result<Vec<DetectedAnomaly>, ScanError> {
    let (period_start, period_end) = lookback_period(now, entry);
    let raw = runner
        .run_time_series(
            entry.measure.clone(),
            entry.time_dimension.clone(),
            entry.granularity.airlayer_str().to_string(),
            (
                period_start.format("%Y-%m-%d").to_string(),
                period_end.format("%Y-%m-%d").to_string(),
            ),
            monitor_filters_to_query_filters(&entry.filters),
        )
        .await?;

    let raw_observations = parse_observations(&raw)?;
    if raw_observations.is_empty() {
        return Ok(vec![]);
    }
    // Zero-fill gaps so MSTL receives a uniformly-spaced array. Sparse series
    // (e.g. no orders on a particular day) would otherwise shift the seasonal
    // alignment and produce false positives or missed anomalies.
    let observations = fill_gaps(raw_observations, entry.granularity);

    let test_window = pick_test_window(&observations, entry.granularity);
    let inputs = DetectInputs {
        series: &observations,
        seasonal_periods: entry.effective_seasonality(),
        test_window,
        sensitivity: entry.sensitivity,
        interval_level: 0.95,
    };
    let mut anomalies = run_detect(inputs)?;
    anomalies.retain(|a| match entry.direction {
        Direction::Both => true,
        Direction::Decrease => a.residual < 0.0,
        Direction::Increase => a.residual > 0.0,
    });
    Ok(anomalies)
}

/// Derive the (start, end) period to query from `now` and the entry's
/// declared lookback. End is `now` (inclusive); start is `now - lookback`.
/// Convert [`MonitorFilter`]s to the airlayer `QueryFilter` format.
/// Each filter becomes an `equals` clause; multiple filters are ANDed by
/// airlayer when passed as a flat list.
fn monitor_filters_to_query_filters(
    filters: &[MonitorFilter],
) -> Vec<airlayer::engine::query::QueryFilter> {
    use airlayer::engine::query::{FilterOperator, QueryFilter};
    filters
        .iter()
        .map(|f| QueryFilter {
            member: Some(f.member.clone()),
            operator: Some(FilterOperator::Equals),
            values: f.values.clone(),
            and: None,
            or: None,
        })
        .collect()
}

/// Clamp the query window end to the last complete period boundary so a
/// mid-period scan never includes an incomplete bucket in detection.
/// Day  → start of today     (excludes today's partial data)
/// Week → start of this ISO week Monday (excludes the current partial week)
/// Month → start of this month's 1st   (excludes the current partial month)
fn lookback_period(now: DateTime<Utc>, entry: &MonitorEntry) -> (DateTime<Utc>, DateTime<Utc>) {
    use chrono::Datelike;
    let today = now.date_naive();
    let end_date: NaiveDate = match entry.granularity {
        Granularity::Day => today,
        Granularity::Week => {
            let days_from_monday = today.weekday().num_days_from_monday();
            today - Duration::days(days_from_monday as i64)
        }
        Granularity::Month => {
            NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today)
        }
    };
    let end = end_date
        .and_hms_opt(0, 0, 0)
        .and_then(|dt| Utc.from_local_datetime(&dt).single())
        .unwrap_or(now);
    let lookback = effective_lookback(entry);
    (end - lookback, end)
}

/// Bump the user-declared lookback when the granularity is coarse so the
/// detector has enough cycles to fit (MSTL needs ≥2 periods per seasonality).
fn effective_lookback(entry: &MonitorEntry) -> Duration {
    let base = Duration::days(entry.lookback_days as i64);
    let max_period = entry.effective_seasonality().into_iter().max().unwrap_or(1) as i64;
    let floor = match entry.granularity {
        Granularity::Day => Duration::days(max_period * 3),
        Granularity::Week => Duration::days(max_period * 3 * 7),
        Granularity::Month => Duration::days(max_period * 3 * 31),
    };
    if base > floor { base } else { floor }
}

/// Convert raw `(timestamp_string, value)` rows into [`Observation`]s.
/// airlayer returns timestamps as `YYYY-MM-DD` for daily granularity and
/// `YYYY-MM-DD HH:MM:SS` for sub-daily; both parse cleanly.
fn parse_observations(rows: &[(String, f64)]) -> Result<Vec<Observation>, ScanError> {
    let mut out = Vec::with_capacity(rows.len());
    for (ts, value) in rows {
        let parsed = parse_timestamp(ts)?;
        out.push(Observation {
            timestamp: parsed,
            value: *value,
        });
    }
    Ok(out)
}

fn parse_timestamp(ts: &str) -> Result<DateTime<Utc>, ScanError> {
    if let Ok(d) = NaiveDate::parse_from_str(ts, "%Y-%m-%d") {
        return Ok(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap()));
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S") {
        return Ok(Utc.from_utc_datetime(&dt));
    }
    // Try RFC3339 (e.g. ClickHouse / Snowflake variants)
    DateTime::parse_from_rfc3339(ts)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|source| ScanError::ParseTimestamp {
            ts: ts.to_string(),
            source,
        })
}

/// Zero-fill missing periods so the series is uniformly spaced.
///
/// MSTL treats the array as uniformly spaced regardless of the timestamps.
/// If the warehouse returns sparse data (no rows for inactive days/weeks),
/// index-based seasonal alignment is wrong. Inserting zeros for missing
/// buckets restores correct alignment without needing to touch the algorithm.
fn fill_gaps(obs: Vec<Observation>, granularity: Granularity) -> Vec<Observation> {
    use std::collections::HashMap;
    if obs.len() < 2 {
        return obs;
    }
    let by_date: HashMap<NaiveDate, f64> = obs
        .iter()
        .map(|o| (o.timestamp.date_naive(), o.value))
        .collect();
    let first = obs.first().unwrap().timestamp.date_naive();
    let last = obs.last().unwrap().timestamp.date_naive();
    let mut out = Vec::new();
    let mut cur = first;
    while cur <= last {
        let value = by_date.get(&cur).copied().unwrap_or(0.0);
        out.push(Observation {
            timestamp: Utc.from_utc_datetime(&cur.and_hms_opt(0, 0, 0).unwrap()),
            value,
        });
        cur = advance_period(cur, granularity);
    }
    out
}

fn advance_period(date: NaiveDate, granularity: Granularity) -> NaiveDate {
    use chrono::Datelike;
    match granularity {
        Granularity::Day => date + Duration::days(1),
        Granularity::Week => date + Duration::weeks(1),
        Granularity::Month => if date.month() == 12 {
            NaiveDate::from_ymd_opt(date.year() + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1)
        }
        .unwrap_or(date + Duration::days(31)),
    }
}

/// Choose how many tail observations to evaluate. For daily series this is
/// the last week; for weekly/monthly it's a single bucket. Keeps the inbox
/// signal-dense without flagging entire weeks at once.
fn pick_test_window(observations: &[Observation], granularity: Granularity) -> usize {
    let default = match granularity {
        Granularity::Day => 7,
        Granularity::Week => 1,
        Granularity::Month => 1,
    };
    default.min(observations.len().saturating_sub(1).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Sensitivity;

    #[test]
    fn granularity_filter_skips_non_matching() {
        let all = vec![
            MonitorEntry {
                measure: "a.b".into(),
                time_dimension: "a.t".into(),
                granularity: Granularity::Day,
                lookback_days: 30,
                seasonality: None,
                sensitivity: Sensitivity::Medium,
                label: None,
                filters: vec![],
                group_by: None,
                direction: Direction::Both,
            },
            MonitorEntry {
                measure: "c.d".into(),
                time_dimension: "c.t".into(),
                granularity: Granularity::Week,
                lookback_days: 90,
                seasonality: None,
                sensitivity: Sensitivity::Medium,
                label: None,
                filters: vec![],
                group_by: None,
                direction: Direction::Both,
            },
        ];
        let filtered: Vec<_> = all
            .into_iter()
            .filter(|m| m.granularity == Granularity::Day)
            .collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].measure, "a.b");
    }

    #[test]
    fn lookback_floor_kicks_in_for_short_user_value() {
        let entry = MonitorEntry {
            measure: "x.y".into(),
            time_dimension: "x.t".into(),
            granularity: Granularity::Day,
            lookback_days: 5, // way too short for weekly seasonality
            seasonality: Some(vec![7]),
            sensitivity: Sensitivity::Medium,
            label: None,
            filters: vec![],
            group_by: None,
            direction: Direction::Both,
        };
        let lb = effective_lookback(&entry);
        assert!(
            lb.num_days() >= 7 * 3,
            "expected floor of 21 days, got {lb}"
        );
    }

    #[test]
    fn timestamp_formats() {
        assert!(parse_timestamp("2024-01-15").is_ok());
        assert!(parse_timestamp("2024-01-15 12:34:56").is_ok());
        assert!(parse_timestamp("not a date").is_err());
    }

    #[test]
    fn lookback_period_day_excludes_today() {
        // Mid-day scan: end should be midnight today (= start of today, excludes partial day)
        let now = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 0).unwrap();
        let entry = MonitorEntry {
            measure: "x.y".into(),
            time_dimension: "x.t".into(),
            granularity: Granularity::Day,
            lookback_days: 30,
            seasonality: Some(vec![7]),
            sensitivity: Sensitivity::Medium,
            label: None,
            filters: vec![],
            group_by: None,
            direction: Direction::Both,
        };
        let (_, end) = lookback_period(now, &entry);
        assert_eq!(
            end,
            Utc.with_ymd_and_hms(2024, 6, 15, 0, 0, 0).unwrap(),
            "end should be midnight today, excluding the partial day"
        );
    }

    #[test]
    fn lookback_period_week_excludes_current_week() {
        // Wednesday 2024-06-12: end should be Monday 2024-06-10 (start of current week)
        let now = Utc.with_ymd_and_hms(2024, 6, 12, 10, 0, 0).unwrap();
        let entry = MonitorEntry {
            measure: "x.y".into(),
            time_dimension: "x.t".into(),
            granularity: Granularity::Week,
            lookback_days: 90,
            seasonality: Some(vec![4]),
            sensitivity: Sensitivity::Medium,
            label: None,
            filters: vec![],
            group_by: None,
            direction: Direction::Both,
        };
        let (_, end) = lookback_period(now, &entry);
        assert_eq!(
            end,
            Utc.with_ymd_and_hms(2024, 6, 10, 0, 0, 0).unwrap(),
            "end should be Monday of the current week, excluding the partial week"
        );
    }

    #[test]
    fn lookback_period_month_excludes_current_month() {
        // June 15: end should be June 1 (start of current month)
        let now = Utc.with_ymd_and_hms(2024, 6, 15, 8, 0, 0).unwrap();
        let entry = MonitorEntry {
            measure: "x.y".into(),
            time_dimension: "x.t".into(),
            granularity: Granularity::Month,
            lookback_days: 365,
            seasonality: Some(vec![12]),
            sensitivity: Sensitivity::Medium,
            label: None,
            filters: vec![],
            group_by: None,
            direction: Direction::Both,
        };
        let (_, end) = lookback_period(now, &entry);
        assert_eq!(
            end,
            Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap(),
            "end should be the 1st of the current month, excluding the partial month"
        );
    }

    #[test]
    fn direction_decrease_keeps_drops_removes_spikes() {
        use crate::detect::{DetectedAnomaly, Severity};
        let t = Utc.with_ymd_and_hms(2024, 6, 14, 0, 0, 0).unwrap();
        let drop = DetectedAnomaly {
            timestamp: t,
            observed: 400.0,
            expected: 1000.0,
            lower: 800.0,
            upper: 1200.0,
            residual: -600.0,
            z_score: -3.5,
            severity: Severity::High,
        };
        let spike = DetectedAnomaly {
            timestamp: t,
            observed: 1600.0,
            expected: 1000.0,
            lower: 800.0,
            upper: 1200.0,
            residual: 600.0,
            z_score: 3.5,
            severity: Severity::High,
        };
        let mut anomalies = vec![drop, spike];
        anomalies.retain(|a| match Direction::Decrease {
            Direction::Both => true,
            Direction::Decrease => a.residual < 0.0,
            Direction::Increase => a.residual > 0.0,
        });
        assert_eq!(anomalies.len(), 1);
        assert_eq!(anomalies[0].residual, -600.0);
    }

    #[test]
    fn direction_increase_keeps_spikes_removes_drops() {
        use crate::detect::{DetectedAnomaly, Severity};
        let t = Utc.with_ymd_and_hms(2024, 6, 14, 0, 0, 0).unwrap();
        let drop = DetectedAnomaly {
            timestamp: t,
            observed: 400.0,
            expected: 1000.0,
            lower: 800.0,
            upper: 1200.0,
            residual: -600.0,
            z_score: -3.5,
            severity: Severity::High,
        };
        let spike = DetectedAnomaly {
            timestamp: t,
            observed: 1600.0,
            expected: 1000.0,
            lower: 800.0,
            upper: 1200.0,
            residual: 600.0,
            z_score: 3.5,
            severity: Severity::High,
        };
        let mut anomalies = vec![drop, spike];
        anomalies.retain(|a| match Direction::Increase {
            Direction::Both => true,
            Direction::Decrease => a.residual < 0.0,
            Direction::Increase => a.residual > 0.0,
        });
        assert_eq!(anomalies.len(), 1);
        assert_eq!(anomalies[0].residual, 600.0);
    }
}
