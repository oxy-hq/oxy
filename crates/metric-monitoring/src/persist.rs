//! Persist [`crate::ScanResult`] outcomes to the `metric_anomalies` table.
//!
//! Lives here (not in `oxy-app`) so both the HTTP handler and the cron tick
//! can call into the same upsert path. The unique index on
//! (workspace_id, measure, time_dimension, dimension_key, period_start) means
//! repeat scans update the existing row in place — dismissed rows stay
//! dismissed, others flip back to `new` so a recurring anomaly resurfaces.
//! `dimension_key` is part of the key (migration `m20260604_000001`) so two
//! segments of the same `group_by` monitor cannot clobber each other's rows.
//!
//! Because each scan re-scores a rolling test window, a bucket withheld by
//! `freshness` is **delayed, not lost**: it is scored on a later run and lands
//! as the same single row.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use entity::metric_anomalies::{self, Entity as AnomaliesEntity};
use entity::metric_monitor_coverage::{self, Entity as CoverageEntity};
use sea_orm::ActiveValue::Set;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, DeleteMany, EntityTrait,
    IntoActiveModel, QueryFilter, QueryOrder, Select,
};
use uuid::Uuid;

use crate::MonitorEntry;
use crate::config::MonitorFilter;
use crate::detect::{CONTINUATION_GAP_BUCKETS, Continuation};
use crate::detect::{DetectedAnomaly, Severity};
use crate::service::{
    OpenEvents, ScanResult, SegmentKey, advance_period, resolve_local_midnight, retreat_period,
};

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

/// Persist everything a scan learned: the anomalies it flagged **and** the
/// per-segment coverage that says whether it was scored at all.
///
/// Callers should prefer this over [`upsert_anomalies`] alone. Writing only the
/// anomalies is what left the Monitors tab unable to distinguish "healthy, no
/// anomalies" from "not scoring": a warming-up segment produces neither an
/// anomaly row nor a failure, so without the coverage row it is invisible.
///
/// Returns the number of anomaly rows touched. A coverage write that fails is
/// logged and swallowed — losing a status row must not fail a scan that
/// successfully detected something.
pub async fn persist_scan(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    scan: &ScanResult,
) -> Result<usize, DbErr> {
    let touched = upsert_anomalies(db, workspace_id, scan).await?;
    if let Err(e) = upsert_coverage(db, workspace_id, scan).await {
        tracing::warn!(
            target: "metric_monitoring",
            workspace_id = %workspace_id,
            error = %e,
            "failed to persist monitor coverage; anomalies were still saved"
        );
    }
    Ok(touched)
}

/// Upsert one `metric_monitor_coverage` row per scanned segment, then drop rows
/// for segments that no longer exist.
///
/// The cleanup matters for `group_by` monitors: when a segment disappears (a
/// store closes, a dimension value stops being returned) its row would
/// otherwise linger forever and keep inflating the "N of M segments" the UI
/// reports. Deletes are scoped to the exact
/// (measure, time_dimension, granularity) triples this scan actually covered,
/// so a granularity-filtered scan cannot delete another grain's rows — and a
/// segment that errored is kept rather than read as vanished (see
/// [`prune_keep_lists`]).
pub async fn upsert_coverage(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    scan: &ScanResult,
) -> Result<usize, DbErr> {
    let now = Utc::now();
    let mut touched = 0usize;

    for outcome in &scan.outcomes {
        let entry = &outcome.entry;
        let dim_key = MonitorFilter::key_for(&entry.filters);
        let granularity = entry.granularity.airlayer_str().to_string();
        let filters_json = if entry.filters.is_empty() {
            None
        } else {
            serde_json::to_value(&entry.filters).ok()
        };
        // i32 to match the column; a bucket count can never approach the bound,
        // but saturate rather than wrap if one ever did.
        let measured = i32::try_from(outcome.coverage.measured).unwrap_or(i32::MAX);
        let required = i32::try_from(outcome.coverage.required).unwrap_or(i32::MAX);

        let existing = CoverageEntity::find()
            .filter(metric_monitor_coverage::Column::WorkspaceId.eq(workspace_id))
            .filter(metric_monitor_coverage::Column::Measure.eq(entry.measure.clone()))
            .filter(metric_monitor_coverage::Column::TimeDimension.eq(entry.time_dimension.clone()))
            .filter(metric_monitor_coverage::Column::Granularity.eq(granularity.clone()))
            .filter(metric_monitor_coverage::Column::DimensionKey.eq(dim_key.clone()))
            .one(db)
            .await?;

        if let Some(found) = existing {
            let mut active = found.into_active_model();
            active.granularity = Set(granularity);
            active.filters = Set(filters_json);
            active.label = Set(entry.label.clone());
            active.measured_buckets = Set(measured);
            active.required_buckets = Set(required);
            active.last_scanned_at = Set(now.into());
            active.update(db).await?;
        } else {
            metric_monitor_coverage::ActiveModel {
                id: Set(Uuid::new_v4()),
                workspace_id: Set(workspace_id),
                measure: Set(entry.measure.clone()),
                time_dimension: Set(entry.time_dimension.clone()),
                granularity: Set(granularity),
                dimension_key: Set(dim_key),
                filters: Set(filters_json),
                label: Set(entry.label.clone()),
                measured_buckets: Set(measured),
                required_buckets: Set(required),
                last_scanned_at: Set(now.into()),
            }
            .insert(db)
            .await?;
        }
        touched += 1;
    }

    prune_vanished_segments(db, workspace_id, &prune_keep_lists(scan)).await?;
    Ok(touched)
}

/// Which segments the prune must keep, per (measure, time-dim, granularity)
/// triple this scan covered.
///
/// Two rules, both about not deleting on incomplete information:
///
/// * Only triples with at least one **successful** outcome are pruned at all.
///   A `group_by` monitor whose segment discovery failed reports a single
///   failure for the parent entry; pruning that triple against just that one
///   key would delete every segment's row on a warehouse hiccup.
/// * Within a pruned triple, a segment that **errored** is kept. It has not
///   vanished — this scan simply does not know. No coverage row is written for
///   it either (the numbers would be stale but presented as fresh), so its last
///   known state stands until a scan can speak to it.
fn prune_keep_lists(scan: &ScanResult) -> HashMap<(String, String, String), HashSet<String>> {
    let triple_of = |entry: &MonitorEntry| {
        (
            entry.measure.clone(),
            entry.time_dimension.clone(),
            entry.granularity.airlayer_str().to_string(),
        )
    };

    let mut keep: HashMap<(String, String, String), HashSet<String>> = HashMap::new();
    for outcome in &scan.outcomes {
        keep.entry(triple_of(&outcome.entry))
            .or_default()
            .insert(MonitorFilter::key_for(&outcome.entry.filters));
    }
    for failure in &scan.failures {
        // `if let` rather than `entry()`: a failure must never *introduce* a
        // triple, only protect a key inside one the scan otherwise covered.
        if let Some(keys) = keep.get_mut(&triple_of(&failure.entry)) {
            keys.insert(MonitorFilter::key_for(&failure.entry.filters));
        }
    }
    keep
}

/// The delete for one scanned (measure, time-dim, granularity) triple: every
/// coverage row of that triple whose segment the scan did not produce.
///
/// Split out so the scoping — the subtle part, since this is the only code here
/// that deletes — can be asserted without a database. The triple filters are
/// what stop a granularity-filtered scan from wiping another grain's rows.
fn prune_query(
    workspace_id: Uuid,
    measure: &str,
    time_dimension: &str,
    granularity: &str,
    keys: &HashSet<String>,
) -> DeleteMany<CoverageEntity> {
    // Sorted so the generated statement is stable across runs; `HashSet`
    // iteration order is not.
    let mut keep: Vec<String> = keys.iter().cloned().collect();
    keep.sort();
    CoverageEntity::delete_many()
        .filter(metric_monitor_coverage::Column::WorkspaceId.eq(workspace_id))
        .filter(metric_monitor_coverage::Column::Measure.eq(measure.to_string()))
        .filter(metric_monitor_coverage::Column::TimeDimension.eq(time_dimension.to_string()))
        .filter(metric_monitor_coverage::Column::Granularity.eq(granularity.to_string()))
        .filter(metric_monitor_coverage::Column::DimensionKey.is_not_in(keep))
}

/// Delete coverage rows for segments this scan no longer produced.
async fn prune_vanished_segments(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    seen: &HashMap<(String, String, String), HashSet<String>>,
) -> Result<(), DbErr> {
    for ((measure, time_dimension, granularity), keys) in seen {
        prune_query(workspace_id, measure, time_dimension, granularity, keys)
            .exec(db)
            .await?;
    }
    Ok(())
}

/// Load the tail of every open anomaly event in a workspace, keyed by segment.
///
/// "Open" means not dismissed: dismissing an anomaly is the operator saying the
/// event is not real, so later buckets must argue their own case again rather
/// than inheriting a waived band from it.
///
/// Called once before a scan and handed to [`crate::scan_workspace`], which
/// keeps the scanning crate free of database access.
pub async fn load_open_events(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> Result<OpenEvents, DbErr> {
    let rows = AnomaliesEntity::find()
        .filter(metric_anomalies::Column::WorkspaceId.eq(workspace_id))
        .filter(metric_anomalies::Column::Status.ne("dismissed"))
        .order_by_asc(metric_anomalies::Column::PeriodStart)
        .all(db)
        .await?;

    // Ascending order means the last write per segment wins, leaving each key
    // pointing at that segment's most recent flagged bucket.
    let mut out = OpenEvents::new();
    for row in rows {
        out.insert(
            SegmentKey {
                measure: row.measure,
                time_dimension: row.time_dimension,
                granularity: row.granularity,
                dimension_key: row.dimension_key,
            },
            Continuation {
                last_period: row.period_start.into(),
                is_decrease: row.observed < row.expected,
            },
        );
    }
    Ok(out)
}

/// The segment an anomaly row belongs to, as filter predicates.
///
/// Every lookup in this module scopes by exactly the four columns of
/// [`SegmentKey`] — including `granularity`. Leaving the grain out lets a
/// measure monitored daily *and* weekly cross-link: their buckets share a start
/// instant whenever a week opens on a scanned day, so a grain-blind query
/// returns the other monitor's row.
fn segment_scope(
    workspace_id: Uuid,
    entry: &MonitorEntry,
    dim_key: &str,
) -> Select<AnomaliesEntity> {
    AnomaliesEntity::find()
        .filter(metric_anomalies::Column::WorkspaceId.eq(workspace_id))
        .filter(metric_anomalies::Column::Measure.eq(entry.measure.clone()))
        .filter(metric_anomalies::Column::TimeDimension.eq(entry.time_dimension.clone()))
        .filter(
            metric_anomalies::Column::Granularity.eq(entry.granularity.airlayer_str().to_string()),
        )
        .filter(metric_anomalies::Column::DimensionKey.eq(dim_key.to_string()))
}

/// The earlier bucket that could carry this bucket's event, if any: same
/// segment, still open, within `CONTINUATION_GAP_BUCKETS` of it.
fn event_candidate_query(
    workspace_id: Uuid,
    entry: &MonitorEntry,
    dim_key: &str,
    period_start: DateTime<Utc>,
    earliest: DateTime<Utc>,
) -> Select<AnomaliesEntity> {
    segment_scope(workspace_id, entry, dim_key)
        .filter(metric_anomalies::Column::Status.ne("dismissed"))
        .filter(
            metric_anomalies::Column::PeriodStart
                .lt::<DateTime<chrono::FixedOffset>>(period_start.into()),
        )
        .filter(
            metric_anomalies::Column::PeriodStart
                .gte::<DateTime<chrono::FixedOffset>>(earliest.into()),
        )
        .order_by_desc(metric_anomalies::Column::PeriodStart)
}

/// The one row a re-scan of this bucket must update in place. Mirrors the
/// unique index (workspace, measure, time-dim, granularity, period, dim key).
fn existing_row_query(
    workspace_id: Uuid,
    entry: &MonitorEntry,
    dim_key: &str,
    period_start: DateTime<Utc>,
) -> Select<AnomaliesEntity> {
    segment_scope(workspace_id, entry, dim_key).filter(
        metric_anomalies::Column::PeriodStart
            .eq::<chrono::DateTime<chrono::FixedOffset>>(period_start.into()),
    )
}

/// The event this bucket belongs to: the one an earlier, same-direction,
/// still-open bucket of the same segment already belongs to, if that bucket is
/// close enough to be the same episode; otherwise a fresh event.
async fn resolve_event_id(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    entry: &MonitorEntry,
    dim_key: &str,
    period_start: DateTime<Utc>,
    is_decrease: bool,
) -> Result<Uuid, DbErr> {
    // Walk back `CONTINUATION_GAP_BUCKETS` periods in the monitor's own
    // calendar — a month is not 31 days and a DST week is not 168 hours, so
    // this cannot be done in fixed durations.
    let tz = entry.effective_timezone();
    let mut earliest = period_start.with_timezone(&tz).date_naive();
    for _ in 0..CONTINUATION_GAP_BUCKETS {
        earliest = retreat_period(earliest, entry.granularity);
    }
    let earliest = resolve_local_midnight(earliest, tz);

    let candidate = event_candidate_query(workspace_id, entry, dim_key, period_start, earliest)
        .one(db)
        .await?;

    if let Some(prev) = candidate
        && (prev.observed < prev.expected) == is_decrease
        && let Some(event_id) = prev.event_id
    {
        return Ok(event_id);
    }
    Ok(Uuid::new_v4())
}

async fn upsert_one(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    entry: &MonitorEntry,
    anomaly: &DetectedAnomaly,
) -> Result<(), DbErr> {
    let now = Utc::now();
    let period_start = anomaly.timestamp;
    let period_end = period_end_for(period_start, entry);

    let dim_key = MonitorFilter::key_for(&entry.filters);
    let dim_key_for_event = dim_key.clone();
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

    let is_decrease = anomaly.residual < 0.0;

    let existing = existing_row_query(workspace_id, entry, &dim_key, period_start)
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
        // Keep an event already assigned: re-scoring a bucket must not split a
        // reported event in two. Only a row that never had one gets one now.
        if matches!(
            active.event_id,
            Set(None) | sea_orm::ActiveValue::Unchanged(None)
        ) {
            active.event_id = Set(Some(
                resolve_event_id(
                    db,
                    workspace_id,
                    entry,
                    &dim_key_for_event,
                    period_start,
                    is_decrease,
                )
                .await?,
            ));
        }
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
            event_id: Set(Some(
                resolve_event_id(
                    db,
                    workspace_id,
                    entry,
                    &dim_key_for_event,
                    period_start,
                    is_decrease,
                )
                .await?,
            )),
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

/// The exclusive end of the bucket starting at `start`, computed in the
/// monitor's LOCAL calendar. A local month is not 31 UTC days, and a DST week
/// is not exactly 168 hours, so this must round-trip through the timezone.
fn period_end_for(start: DateTime<Utc>, entry: &MonitorEntry) -> DateTime<Utc> {
    let tz = entry.effective_timezone();
    let local = start.with_timezone(&tz).date_naive();
    let next = advance_period(local, entry.granularity);
    resolve_local_midnight(next, tz)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Direction, Granularity, Sensitivity};
    use chrono::NaiveDate;
    use sea_orm::QueryTrait;

    fn entry_at(granularity: Granularity, tz: &str) -> MonitorEntry {
        MonitorEntry {
            measure: "x.y".into(),
            time_dimension: "x.t".into(),
            granularity,
            lookback_days: 90,
            seasonality: Some(vec![7]),
            sensitivity: Sensitivity::Medium,
            label: None,
            filters: vec![],
            group_by: None,
            direction: Direction::Both,
            timezone: Some(tz.into()),
            freshness: None,
            week_start: None,
        }
    }

    fn sql_of(stmt: sea_orm::Statement) -> String {
        stmt.to_string()
    }

    /// Both row lookups must name the grain, or a measure monitored daily *and*
    /// weekly cross-links: a Monday daily bucket and the weekly bucket opening
    /// that same Monday share a `period_start`, so a grain-blind query returns
    /// the other monitor's row — the daily bucket inherits the weekly event id
    /// (and, via the `existing` lookup, overwrites the weekly row outright).
    #[test]
    fn segment_lookups_are_scoped_to_one_granularity() {
        let workspace_id = Uuid::nil();
        let period_start = NaiveDate::from_ymd_opt(2026, 7, 27)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let earliest = period_start - chrono::Duration::days(3);

        for granularity in [Granularity::Day, Granularity::Week] {
            let entry = entry_at(granularity, "UTC");
            let grain = granularity.airlayer_str();

            let candidate = sql_of(
                event_candidate_query(workspace_id, &entry, "", period_start, earliest)
                    .build(sea_orm::DbBackend::Postgres),
            );
            assert!(
                candidate.contains(&format!(r#""granularity" = '{grain}'"#)),
                "event candidate query is not scoped to {grain}: {candidate}"
            );

            let existing = sql_of(
                existing_row_query(workspace_id, &entry, "", period_start)
                    .build(sea_orm::DbBackend::Postgres),
            );
            assert!(
                existing.contains(&format!(r#""granularity" = '{grain}'"#)),
                "existing-row query is not scoped to {grain}: {existing}"
            );
        }
    }

    fn segment(granularity: Granularity, member: &str, value: &str) -> MonitorEntry {
        let mut entry = entry_at(granularity, "UTC");
        entry.filters = vec![MonitorFilter {
            member: member.into(),
            values: vec![value.into()],
        }];
        entry
    }

    fn outcome(entry: MonitorEntry) -> crate::service::MonitorOutcome {
        crate::service::MonitorOutcome {
            entry,
            anomalies: vec![],
            coverage: crate::service::Coverage {
                measured: 60,
                required: 56,
            },
        }
    }

    fn failure(entry: MonitorEntry) -> crate::service::MonitorFailure {
        crate::service::MonitorFailure {
            entry,
            error: crate::service::ScanError::ParseTimestamp {
                ts: "not-a-date".into(),
                source: "not-a-date".parse::<chrono::NaiveDate>().unwrap_err(),
            },
        }
    }

    /// A segment that errored has not vanished — the scan just can't speak to
    /// it. Deleting its coverage row on a warehouse hiccup flips a "Warming up"
    /// badge to "—" for a cycle, which reads as "this monitor is fine".
    #[test]
    fn a_failed_segment_is_kept_not_pruned() {
        let scan = ScanResult {
            outcomes: vec![outcome(segment(Granularity::Day, "x.store", "1"))],
            failures: vec![failure(segment(Granularity::Day, "x.store", "2"))],
        };

        let keep = prune_keep_lists(&scan);
        let keys = keep
            .get(&("x.y".into(), "x.t".into(), "day".into()))
            .expect("the scanned triple is pruned");
        assert!(
            keys.contains("x.store=2"),
            "the errored segment was left to be pruned: {keys:?}"
        );
        assert!(keys.contains("x.store=1"));
    }

    /// When segment discovery itself fails there is one failure for the parent
    /// entry and no outcomes. Pruning that triple against a single key would
    /// delete every segment's coverage row, so the triple must not be pruned.
    #[test]
    fn a_triple_with_only_failures_is_not_pruned_at_all() {
        let scan = ScanResult {
            outcomes: vec![],
            failures: vec![failure(entry_at(Granularity::Day, "UTC"))],
        };

        assert!(
            prune_keep_lists(&scan).is_empty(),
            "a triple the scan never succeeded on must not be pruned"
        );
    }

    /// The prune is the only code here that deletes, and its scoping is what
    /// keeps a granularity-filtered scan from wiping another grain's coverage:
    /// the `NOT IN (seen keys)` is only safe because the triple pins it to the
    /// exact (measure, time-dim, grain) this scan actually covered.
    #[test]
    fn prune_is_scoped_to_the_scanned_triple() {
        let seen: HashSet<String> = ["store=1".to_string(), "store=2".to_string()]
            .into_iter()
            .collect();
        let sql = sql_of(
            prune_query(Uuid::nil(), "x.y", "x.t", "day", &seen)
                .build(sea_orm::DbBackend::Postgres),
        );

        for predicate in [
            r#""measure" = 'x.y'"#,
            r#""time_dimension" = 'x.t'"#,
            r#""granularity" = 'day'"#,
            r#""dimension_key" NOT IN ('store=1', 'store=2')"#,
        ] {
            assert!(
                sql.contains(predicate),
                "prune lost its {predicate} scope: {sql}"
            );
        }
    }

    #[test]
    fn period_end_for_week_survives_a_spring_forward_gap_in_santiago() {
        // Sunday 2026-08-30 00:00 -04:00 (Chile Standard Time) = 04:00 UTC.
        // `advance_period` steps a Week monitor 7 days to 2026-09-06, which
        // has no local midnight in America/Santiago (it springs forward
        // 00:00 -> 01:00 -03:00 that day). The granularity-blind
        // `start + 1 day` fallback this replaced would report period_end
        // ~1 day after start instead of ~7 — silently corrupting the stored
        // anomaly row for every Week/Month monitor that hits a DST gap.
        let start = NaiveDate::from_ymd_opt(2026, 8, 30)
            .unwrap()
            .and_hms_opt(4, 0, 0)
            .unwrap()
            .and_utc();
        let entry = entry_at(Granularity::Week, "America/Santiago");
        // Chile's 2026 spring-forward gap is 2026-09-06 00:00 -> 01:00, at the
        // instant 04:00 UTC, so `resolve_local_midnight` lands on the first
        // valid local time after midnight: 01:00 -03:00 = 04:00 UTC. Asserted
        // exactly — a `>= 6 days` bound would also pass for a 6-day answer and
        // so could not catch the off-by-one-day class this test guards.
        let end = period_end_for(start, &entry);
        assert_eq!(
            end,
            NaiveDate::from_ymd_opt(2026, 9, 6)
                .unwrap()
                .and_hms_opt(4, 0, 0)
                .unwrap()
                .and_utc(),
            "expected the week boundary at Chile's post-gap local midnight"
        );
    }
}
