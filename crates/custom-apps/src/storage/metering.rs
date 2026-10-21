//! GB-month metering from the sample series.
//!
//! ## Why an average and not a peak
//!
//! Storage is a *rate* — bytes held over time — so the billable quantity is the
//! integral, not any single reading. Billing the peak would charge an app that
//! wrote a 5 GB export and deleted it an hour later for a full month of 5 GB.
//! Billing the end-of-period value would let anyone zero their invoice by
//! deleting on the last day. Vercel Blob resolves this the same way: snapshot
//! periodically, average over the month.
//!
//! ## Time-weighted, not a plain mean of samples
//!
//! A plain mean over an app's own samples silently over-bills a **mid-period**
//! app: an app created on the 28th has only a few samples, all at its real size,
//! and averaging just those reports that size as if it had been held all month.
//!
//! So each sample is treated as a step held until the next one (and the last
//! until the period ends), the area under that step function is summed, and the
//! total is divided by the **whole period**. An app present for half the period
//! contributes half its bytes, which is what actually happened.
//!
//! To get the value at `period_start` right, the sample immediately *before* the
//! window is fetched too — otherwise every period would under-count its opening
//! stretch, and a long-lived app would be billed as if it materialized at its
//! first in-window sample.
//!
//! ## Samples are exact-only
//!
//! The sweeper only appends a sample when the walk completed
//! (`measure_status = ok`). A partial walk is a floor, and averaging floors would
//! under-bill silently — the one direction of error nobody notices until an audit.

use std::collections::HashMap;

use chrono::{DateTime, Datelike, FixedOffset, Utc};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, FromQueryResult, QueryFilter,
    QueryOrder,
};
use uuid::Uuid;

use entity::app_storage_usage_samples;
use entity::prelude::{AppStorageUsage, AppStorageUsageSamples};

const BYTES_PER_GIB: f64 = (1024 * 1024 * 1024) as f64;

/// How far before `period_start` to look for the carry-forward reading that sets
/// the opening value. Long enough that any app the sweeper has touched recently
/// is covered, short enough that metering one month never scans a year of
/// samples.
const SAMPLE_LOOKBACK_DAYS: i64 = 45;

/// One app's metered contribution over a period.
#[derive(Debug, Clone, PartialEq)]
pub struct AppMeter {
    pub app_id: Uuid,
    /// Time-weighted mean bytes held across the whole period.
    pub average_bytes: f64,
    /// `average_bytes` expressed in GiB — the billable quantity.
    pub gib_month: f64,
    /// Samples that informed this figure. Zero means "no data", which is NOT
    /// the same as zero usage and must never be billed as such.
    pub sample_count: usize,
}

/// An org's metered usage over a period.
#[derive(Debug, Clone, PartialEq)]
pub struct OrgMeter {
    pub org_id: Uuid,
    pub period_start: DateTime<FixedOffset>,
    pub period_end: DateTime<FixedOffset>,
    pub gib_month: f64,
    pub average_bytes: f64,
    pub apps: Vec<AppMeter>,
    /// Apps in the org with no samples in or before the period. Their usage is
    /// **excluded**, so a non-empty list means this figure is an under-count and
    /// must not be invoiced without a look.
    pub apps_without_samples: Vec<Uuid>,
}

/// A `(timestamp, bytes)` reading.
type Sample = (DateTime<FixedOffset>, i64);

/// Integrate a step function over `[start, end)` and divide by its width.
///
/// `samples` must be sorted ascending. Readings at or before `start` establish
/// the opening value; readings after `end` are ignored.
pub fn time_weighted_average_bytes(
    samples: &[Sample],
    start: DateTime<FixedOffset>,
    end: DateTime<FixedOffset>,
) -> f64 {
    let total_secs = (end - start).num_seconds();
    if total_secs <= 0 || samples.is_empty() {
        return 0.0;
    }

    // Opening value: the last reading at or before `start`. Without this an app
    // that existed all period but wasn't sampled exactly at `start` loses its
    // opening stretch.
    let mut current: Option<i64> = samples
        .iter()
        .rev()
        .find(|(t, _)| *t <= start)
        .map(|(_, b)| *b);
    let mut cursor = start;
    let mut area: f64 = 0.0;

    for &(at, bytes) in samples.iter() {
        if at <= start {
            continue;
        }
        let boundary = at.min(end);
        if let Some(held) = current {
            let secs = (boundary - cursor).num_seconds().max(0);
            area += held as f64 * secs as f64;
        }
        cursor = boundary;
        current = Some(bytes);
        if cursor >= end {
            break;
        }
    }

    // The final reading holds until the period ends.
    if let Some(held) = current
        && cursor < end
    {
        area += held as f64 * (end - cursor).num_seconds() as f64;
    }

    area / total_secs as f64
}

/// Meter one org over `[period_start, period_end)`.
pub async fn meter_org(
    db: &DatabaseConnection,
    org_id: Uuid,
    period_start: DateTime<FixedOffset>,
    period_end: DateTime<FixedOffset>,
) -> Result<OrgMeter, sea_orm::DbErr> {
    use entity::app_storage_usage;

    let app_ids: Vec<Uuid> = AppStorageUsage::find()
        .filter(app_storage_usage::Column::OrgId.eq(org_id))
        .all(db)
        .await?
        .into_iter()
        .map(|r| r.app_id)
        .collect();

    if app_ids.is_empty() {
        return Ok(OrgMeter {
            org_id,
            period_start,
            period_end,
            gib_month: 0.0,
            average_bytes: 0.0,
            apps: Vec::new(),
            apps_without_samples: Vec::new(),
        });
    }

    // One query for the whole org, bounded at BOTH ends.
    //
    // The upper bound is the period end. The lower bound reaches back past
    // `period_start` on purpose — that is what pulls in the pre-window reading
    // the opening value depends on — but it is still a bound: without one this
    // would scan every app's full 400-day retained history to meter a single
    // month. An app with no sample in the whole lookback is pathological (the
    // sweeper has not reached it in a month) and correctly lands in
    // `apps_without_samples` rather than being billed off stale data.
    let lookback = period_start - chrono::Duration::days(SAMPLE_LOOKBACK_DAYS);
    let rows = AppStorageUsageSamples::find()
        .filter(app_storage_usage_samples::Column::AppId.is_in(app_ids.clone()))
        .filter(app_storage_usage_samples::Column::MeasuredAt.gte(lookback))
        .filter(app_storage_usage_samples::Column::MeasuredAt.lt(period_end))
        .order_by_asc(app_storage_usage_samples::Column::MeasuredAt)
        .all(db)
        .await?;

    let mut by_app: HashMap<Uuid, Vec<Sample>> = HashMap::new();
    for row in rows {
        by_app
            .entry(row.app_id)
            .or_default()
            .push((row.measured_at, row.bytes));
    }

    let mut apps = Vec::new();
    let mut apps_without_samples = Vec::new();
    let mut total_average = 0.0;
    for app_id in app_ids {
        let Some(samples) = by_app.get(&app_id) else {
            apps_without_samples.push(app_id);
            continue;
        };
        let average_bytes = time_weighted_average_bytes(samples, period_start, period_end);
        total_average += average_bytes;
        apps.push(AppMeter {
            app_id,
            average_bytes,
            gib_month: average_bytes / BYTES_PER_GIB,
            sample_count: samples.len(),
        });
    }
    apps.sort_by(|a, b| b.average_bytes.total_cmp(&a.average_bytes));

    Ok(OrgMeter {
        org_id,
        period_start,
        period_end,
        gib_month: total_average / BYTES_PER_GIB,
        average_bytes: total_average,
        apps,
        apps_without_samples,
    })
}

/// Meter the current calendar month to now — the default the admin console shows.
pub async fn meter_org_month_to_date(
    db: &DatabaseConnection,
    org_id: Uuid,
) -> Result<OrgMeter, sea_orm::DbErr> {
    let now = Utc::now();
    let start = now
        .date_naive()
        .with_day(1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|d| d.and_utc())
        .unwrap_or(now);
    meter_org(db, org_id, start.into(), now.into()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const GIB: i64 = 1024 * 1024 * 1024;

    /// A June instant. June has 30 days — `t(31, _)` would be `LocalResult::None`
    /// and panic, which is why the period ends at [`july_1`] rather than "day 31".
    fn t(day: u32, hour: u32) -> DateTime<FixedOffset> {
        Utc.with_ymd_and_hms(2026, 6, day, hour, 0, 0)
            .unwrap()
            .into()
    }

    fn july_1() -> DateTime<FixedOffset> {
        Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap().into()
    }

    /// All of June: 30 days, half-open `[Jun 1, Jul 1)`.
    fn period() -> (DateTime<FixedOffset>, DateTime<FixedOffset>) {
        (t(1, 0), july_1())
    }

    #[test]
    fn a_constant_size_held_all_period_averages_to_itself() {
        let (start, end) = period();
        let samples = vec![
            (t(1, 0), 10 * GIB),
            (t(15, 0), 10 * GIB),
            (t(30, 0), 10 * GIB),
        ];
        let avg = time_weighted_average_bytes(&samples, start, end);
        assert!((avg - (10 * GIB) as f64).abs() < 1.0, "got {avg}");
    }

    #[test]
    fn a_spike_is_billed_for_its_duration_not_its_height() {
        // 0 bytes all month, 30 GiB for the final day. Billing the PEAK would
        // charge 30 GiB-months; the honest answer is ~1 GiB-month.
        let (start, end) = period();
        let samples = vec![(t(1, 0), 0), (t(30, 0), 30 * GIB)];
        let avg = time_weighted_average_bytes(&samples, start, end);
        let gib = avg / BYTES_PER_GIB;
        assert!((gib - 1.0).abs() < 0.05, "expected ~1 GiB-month, got {gib}");
    }

    #[test]
    fn deleting_everything_at_the_end_does_not_zero_the_bill() {
        // 30 GiB held for 29 days then deleted. End-of-period billing would
        // charge nothing, which is the obvious way to game a naive meter.
        let (start, end) = period();
        let samples = vec![(t(1, 0), 30 * GIB), (t(30, 0), 0)];
        let gib = time_weighted_average_bytes(&samples, start, end) / BYTES_PER_GIB;
        assert!(gib > 28.0, "expected ~29 GiB-months, got {gib}");
    }

    #[test]
    fn a_mid_period_app_is_weighted_by_the_time_it_existed() {
        // Created on the 16th at 10 GiB: half a month, so ~5 GiB-months. A plain
        // mean over its own samples would have said 10 — the over-bill this
        // whole function exists to avoid.
        let (start, end) = period();
        let samples = vec![(t(16, 0), 10 * GIB), (t(25, 0), 10 * GIB)];
        let gib = time_weighted_average_bytes(&samples, start, end) / BYTES_PER_GIB;
        assert!((gib - 5.0).abs() < 0.2, "expected ~5 GiB-months, got {gib}");
    }

    #[test]
    fn a_reading_before_the_window_sets_the_opening_value() {
        // Last measured in May, unchanged since. The whole period is covered by
        // that carry-forward; without it the opening stretch would be free.
        let (start, end) = period();
        let before: DateTime<FixedOffset> =
            Utc.with_ymd_and_hms(2026, 5, 20, 0, 0, 0).unwrap().into();
        let samples = vec![(before, 8 * GIB)];
        let gib = time_weighted_average_bytes(&samples, start, end) / BYTES_PER_GIB;
        assert!((gib - 8.0).abs() < 0.01, "expected 8 GiB-months, got {gib}");
    }

    #[test]
    fn readings_after_the_window_are_ignored() {
        let (start, end) = period();
        let after: DateTime<FixedOffset> =
            Utc.with_ymd_and_hms(2026, 7, 15, 0, 0, 0).unwrap().into();
        let samples = vec![(t(1, 0), 2 * GIB), (after, 100 * GIB)];
        let gib = time_weighted_average_bytes(&samples, start, end) / BYTES_PER_GIB;
        assert!(
            (gib - 2.0).abs() < 0.01,
            "next period's data leaked in: {gib}"
        );
    }

    #[test]
    fn no_samples_meters_zero_rather_than_panicking() {
        let (start, end) = period();
        assert_eq!(time_weighted_average_bytes(&[], start, end), 0.0);
    }

    #[test]
    fn an_inverted_or_empty_period_is_zero_not_a_divide_by_zero() {
        let samples = vec![(t(1, 0), GIB)];
        assert_eq!(time_weighted_average_bytes(&samples, t(1, 0), t(1, 0)), 0.0);
        assert_eq!(
            time_weighted_average_bytes(&samples, july_1(), t(1, 0)),
            0.0
        );
    }

    #[test]
    fn irregular_sampling_does_not_bias_the_average() {
        // Ten readings clustered in one day plus one for the rest of the month.
        // A plain mean would be dominated by the cluster; time-weighting is not.
        let (start, end) = period();
        let mut samples: Vec<Sample> = (0..10).map(|h| (t(2, h), 100 * GIB)).collect();
        samples.insert(0, (t(1, 0), 0));
        samples.push((t(3, 0), 0));
        let gib = time_weighted_average_bytes(&samples, start, end) / BYTES_PER_GIB;
        // 100 GiB is held from Jun 2 00:00 until the Jun 3 reading — one day of
        // thirty, so 100/30 ≈ 3.33. A plain mean of these twelve readings would
        // say (100×10)/12 ≈ 83: the cluster would decide the invoice.
        assert!((gib - 3.33).abs() < 0.05, "expected ~3.33, got {gib}");
    }
}

// ── Daily history (charts) ───────────────────────────────────────────────────

/// One day's total, as held at the end of that day.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyPoint {
    /// `YYYY-MM-DD`, UTC.
    pub date: String,
    pub bytes: i64,
    pub object_count: i64,
}

/// Daily totals over the trailing `days`, for the usage-over-time chart.
///
/// Storage is a **level**, not a flow, so each day reports the value *held* at
/// that day's end — the last reading at or before it, carried forward. Summing
/// the day's samples instead would make a day with two measurements look twice
/// as large, and a day with none look empty; both are artifacts of the sweep
/// schedule rather than anything the tenant did.
///
/// Apps are carried forward **independently** and then summed, so an app the
/// sweeper didn't reach on some day still contributes its last known size rather
/// than dropping the fleet total into a trough.
pub fn daily_series(
    per_app: &HashMap<Uuid, Vec<(DateTime<FixedOffset>, i64, i64)>>,
    start: DateTime<FixedOffset>,
    days: i64,
) -> Vec<DailyPoint> {
    // One forward cursor per app, advanced as the days advance.
    //
    // Both sequences ascend, so each sample is visited once across the whole
    // window: O(apps x (days + samples)). Re-scanning each app's vector per day
    // — `samples.iter().rev().find(...)` — is O(apps x days^2) instead, which at
    // the 365-day ceiling and a four-figure fleet is tens of millions of
    // comparisons on a route with no server-side cache.
    let mut cursors: Vec<(&[(DateTime<FixedOffset>, i64, i64)], usize)> =
        per_app.values().map(|s| (s.as_slice(), 0usize)).collect();

    let mut out = Vec::with_capacity(days as usize);
    for day in 0..days {
        let day_end = start + chrono::Duration::days(day + 1);
        let (mut bytes, mut objects) = (0i64, 0i64);
        for (samples, cursor) in cursors.iter_mut() {
            // Advance past every reading that closed before this day ended; the
            // last one stepped over is the value held when the day closed.
            while *cursor < samples.len() && samples[*cursor].0 < day_end {
                *cursor += 1;
            }
            if *cursor > 0 {
                let (_, b, o) = samples[*cursor - 1];
                bytes += b;
                objects += o;
            }
        }
        out.push(DailyPoint {
            date: (start + chrono::Duration::days(day))
                .format("%Y-%m-%d")
                .to_string(),
            bytes,
            object_count: objects,
        });
    }
    out
}

/// Load the samples [`daily_series`] needs, bounded at both ends.
///
/// Reaches back past `start` for the same reason [`meter_org`] does: without a
/// carry-forward reading, every app looks like it materialized at its first
/// in-window sample and the chart opens with a false ramp.
///
/// The upper bound is explicit rather than left to `daily_series` ignoring later
/// rows: a window ending in the past would otherwise load every sample since,
/// which is the whole retained series for the common case of a short window.
/// `orgs: Some(..)` narrows the series to a bounded grant's reach. The samples
/// table is keyed by app alone and carries no `org_id`, so unlike the rollup this
/// one cannot filter in place — it goes through `apps`, which is the authority on
/// which org owns an app. (The rollup would also work and needs no subquery, but
/// it only has rows for *measured* apps; deriving reach from it would silently
/// widen scope for an app whose rollup row is missing.)
pub async fn load_history(
    db: &DatabaseConnection,
    app_id: Option<Uuid>,
    orgs: Option<&[Uuid]>,
    days: i64,
) -> Result<Vec<DailyPoint>, sea_orm::DbErr> {
    let now = Utc::now();
    let start: DateTime<FixedOffset> = (now - chrono::Duration::days(days - 1))
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|d| d.and_utc())
        .unwrap_or(now)
        .into();
    let lookback = start - chrono::Duration::days(SAMPLE_LOOKBACK_DAYS);

    let window_end = start + chrono::Duration::days(days);

    // One row per (app, day) — the sweeper appends a sample every tick (15 min by
    // default, so ~96/app/day), and the chart draws one point per day. Fetching
    // the raw rows means a 10-app fleet at the 365-day ceiling deserializes
    // ~390k rows to render 365 points, and even the 30-day default pulls ~35k on
    // an uncached route.
    //
    // Collapsing in SQL cuts what is **transferred and deserialized** to
    // `apps × days`. Postgres still scans the window and now sorts it — the
    // `date_trunc` expression matches no index, and the `(app_id, measured_at)`
    // primary key gives an incremental-sort prefix at best — so this is a
    // client-side win, not an index-assisted one. Still worth it: the row count
    // is what crosses the wire and allocates.
    //
    // No rendered value changes: `daily_series` already reduces each day to its
    // closing reading, which is exactly the row `ORDER BY … measured_at DESC`
    // keeps inside each `DISTINCT ON` group.
    // Postgres-only: `DISTINCT ON` has no query-builder equivalent, and the `$n`
    // placeholders are Postgres syntax. Fine here — the workspace is
    // PostgreSQL-only — but this is the one statement in the module that would
    // need rewriting for another backend.
    //
    // `AT TIME ZONE 'UTC'` is load-bearing, not decoration. `measured_at` is
    // `timestamptz`, and bare `date_trunc('day', …)` buckets it by the
    // connection's `TimeZone` GUC, while `daily_series` walks days from a UTC
    // midnight. On a deployment whose Postgres is not `TimeZone = 'UTC'` the two
    // would disagree: at UTC+7 the row kept for a bucket is the latest sample
    // ≤ 17:00 UTC, so a UTC day's real closing reading at 23:45 is dropped and
    // the chart silently reports the 16:45 value as the close.
    // Placeholders are numbered as the clauses are pushed, so the two optional
    // filters cannot be written as fixed `$3`/`$4` — an org scope with no app id
    // would then reference a `$4` that was never bound.
    let mut values: Vec<sea_orm::Value> = vec![lookback.into(), window_end.into()];
    let mut filters = String::new();
    if let Some(id) = app_id {
        values.push(id.into());
        filters.push_str(&format!(" AND app_id = ${}", values.len()));
    }
    if let Some(orgs) = orgs {
        // `= ANY($n)` over a uuid[] rather than an expanded `IN (…)` list: one
        // bound value, so the statement shape stays constant however many orgs a
        // grant names, and an empty scope correctly matches nothing.
        values.push(orgs.to_vec().into());
        filters.push_str(&format!(
            " AND app_id IN (SELECT id FROM apps WHERE org_id = ANY(${}))",
            values.len()
        ));
    }
    let sql = format!(
        "SELECT DISTINCT ON (app_id, date_trunc('day', measured_at AT TIME ZONE 'UTC')) \
           app_id, measured_at, bytes, object_count \
         FROM app_storage_usage_samples \
         WHERE measured_at >= $1 AND measured_at < $2{filters} \
         ORDER BY app_id, date_trunc('day', measured_at AT TIME ZONE 'UTC'), measured_at DESC"
    );
    let rows = DailySample::find_by_statement(sea_orm::Statement::from_sql_and_values(
        db.get_database_backend(),
        &sql,
        values,
    ))
    .all(db)
    .await?;

    let mut per_app: HashMap<Uuid, Vec<(DateTime<FixedOffset>, i64, i64)>> = HashMap::new();
    for row in rows {
        per_app
            .entry(row.app_id)
            .or_default()
            .push((row.measured_at, row.bytes, row.object_count));
    }
    // `DISTINCT ON` dictates its own ORDER BY, so restore the ascending order
    // `daily_series`'s forward cursor depends on. At most `days` entries per app.
    for samples in per_app.values_mut() {
        samples.sort_by_key(|(t, _, _)| *t);
    }
    Ok(daily_series(&per_app, start, days))
}

/// Row shape for the collapsed daily query.
#[derive(Debug, sea_orm::FromQueryResult)]
struct DailySample {
    app_id: Uuid,
    measured_at: DateTime<FixedOffset>,
    bytes: i64,
    object_count: i64,
}

#[cfg(test)]
mod history_tests {
    use super::*;
    use chrono::TimeZone;

    const MB: i64 = 1024 * 1024;

    fn day(d: u32) -> DateTime<FixedOffset> {
        Utc.with_ymd_and_hms(2026, 6, d, 12, 0, 0).unwrap().into()
    }
    fn midnight(d: u32) -> DateTime<FixedOffset> {
        Utc.with_ymd_and_hms(2026, 6, d, 0, 0, 0).unwrap().into()
    }

    #[test]
    fn a_day_with_no_sample_carries_the_previous_value_forward() {
        // The sweeper measures in batches, so most apps are NOT sampled daily.
        // Treating a gap as zero would draw a sawtooth that never happened.
        let mut per_app = HashMap::new();
        per_app.insert(Uuid::from_u128(1), vec![(day(1), 10 * MB, 5)]);
        let series = daily_series(&per_app, midnight(1), 4);
        assert_eq!(series.len(), 4);
        assert!(series.iter().all(|p| p.bytes == 10 * MB), "{series:?}");
    }

    #[test]
    fn two_samples_in_one_day_do_not_double_the_total() {
        // Storage is a level, not a flow — summing a day's readings would report
        // 30 MB for an app that never held more than 20.
        let mut per_app = HashMap::new();
        per_app.insert(
            Uuid::from_u128(1),
            vec![
                (
                    Utc.with_ymd_and_hms(2026, 6, 1, 6, 0, 0).unwrap().into(),
                    10 * MB,
                    1,
                ),
                (
                    Utc.with_ymd_and_hms(2026, 6, 1, 18, 0, 0).unwrap().into(),
                    20 * MB,
                    2,
                ),
            ],
        );
        let series = daily_series(&per_app, midnight(1), 1);
        assert_eq!(
            series[0].bytes,
            20 * MB,
            "should be the day's CLOSING value"
        );
    }

    #[test]
    fn apps_are_carried_forward_independently_then_summed() {
        // App B is measured only on day 1. On day 3 the fleet total must still
        // include it, not dip because that app's row was stale.
        let mut per_app = HashMap::new();
        per_app.insert(
            Uuid::from_u128(1),
            vec![(day(1), 10 * MB, 1), (day(3), 12 * MB, 1)],
        );
        per_app.insert(Uuid::from_u128(2), vec![(day(1), 5 * MB, 1)]);
        let series = daily_series(&per_app, midnight(1), 3);
        assert_eq!(series[0].bytes, 15 * MB);
        assert_eq!(series[1].bytes, 15 * MB, "no dip on an unsampled day");
        assert_eq!(series[2].bytes, 17 * MB);
    }

    #[test]
    fn an_app_with_no_reading_yet_contributes_nothing_rather_than_panicking() {
        let mut per_app = HashMap::new();
        per_app.insert(Uuid::from_u128(1), Vec::new());
        let series = daily_series(&per_app, midnight(1), 2);
        assert!(series.iter().all(|p| p.bytes == 0));
    }

    #[test]
    fn a_cursor_never_runs_ahead_of_the_day_it_is_reporting() {
        // The forward-cursor rewrite is easy to get off by one: a cursor that
        // steps one sample too far reports tomorrow's value today, which is
        // invisible on a flat series and wrong on every real one.
        let mut per_app = HashMap::new();
        per_app.insert(
            Uuid::from_u128(1),
            vec![
                (day(1), 1 * MB, 1),
                (day(2), 2 * MB, 2),
                (day(3), 3 * MB, 3),
            ],
        );
        let series = daily_series(&per_app, midnight(1), 3);
        assert_eq!(
            series.iter().map(|p| p.bytes).collect::<Vec<_>>(),
            vec![1 * MB, 2 * MB, 3 * MB],
            "each day must report its OWN closing value"
        );
    }

    #[test]
    fn a_sample_before_the_window_still_sets_the_opening_day() {
        // The cursor starts at 0, so a reading older than `start` must still be
        // stepped over — otherwise day 0 reports nothing for a long-lived app.
        let mut per_app = HashMap::new();
        let earlier: DateTime<FixedOffset> =
            Utc.with_ymd_and_hms(2026, 5, 20, 0, 0, 0).unwrap().into();
        per_app.insert(Uuid::from_u128(1), vec![(earlier, 7 * MB, 3)]);
        let series = daily_series(&per_app, midnight(1), 2);
        assert!(series.iter().all(|p| p.bytes == 7 * MB), "{series:?}");
    }

    #[test]
    fn dates_are_contiguous_and_ascending() {
        // A chart's x-axis assumes this; a gap would silently compress the plot.
        let series = daily_series(&HashMap::new(), midnight(1), 5);
        let dates: Vec<&str> = series.iter().map(|p| p.date.as_str()).collect();
        assert_eq!(
            dates,
            [
                "2026-06-01",
                "2026-06-02",
                "2026-06-03",
                "2026-06-04",
                "2026-06-05"
            ]
        );
    }
}
