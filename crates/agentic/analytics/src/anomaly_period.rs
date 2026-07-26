//! Seasonality-aware comparison period for anomaly root-cause explains.
//!
//! The explain feature diffs the anomalous bucket against a "normal" baseline
//! one full seasonal cycle earlier — the same phase in the previous cycle
//! (e.g. the same weekday last week for a daily/weekly-seasonal monitor).
//! Comparing against the immediately-preceding bucket instead pits Monday
//! against Sunday and surfaces day-of-week noise as if it were the anomaly —
//! the "explained by comparing to the weekend" bug.
//!
//! The seasonal period comes from the monitor's detection config, snapshotted
//! onto the anomaly row at scan time. When it is absent (rows detected before
//! the column existed) we fall back to the granularity's default cycle, which
//! matches the detector's own defaults in
//! `metric_monitoring::config::Granularity::default_seasonality`.

use chrono::{DateTime, Duration, FixedOffset, Months, NaiveDate};

/// Default seasonal cycle length (in units of `granularity`) when the anomaly
/// row carries no persisted seasonality. Mirrors the detector's per-granularity
/// defaults: daily → weekly (7), weekly → monthly-ish (4), monthly → yearly
/// (12).
pub fn default_seasonal_period(granularity: &str) -> u32 {
    match granularity {
        "week" => 4,
        "month" => 12,
        "quarter" => 4,
        // "day" (and anything unexpected) → weekly cycle.
        _ => 7,
    }
}

/// Resolve the seasonal period to compare against: the persisted value when
/// present and positive, otherwise the granularity default.
pub fn resolve_seasonal_period(seasonal_period: Option<i32>, granularity: &str) -> u32 {
    match seasonal_period {
        Some(p) if p > 0 => p as u32,
        _ => default_seasonal_period(granularity),
    }
}

/// Shift a naive date back by `periods` units of `granularity` (calendar-correct
/// for month/quarter so "one cycle ago" lands on the same day-of-month).
pub fn shift_date_back(date: NaiveDate, granularity: &str, periods: u32) -> NaiveDate {
    match granularity {
        "week" => date - Duration::days(7 * periods as i64),
        "month" => date
            .checked_sub_months(Months::new(periods))
            .unwrap_or(date),
        "quarter" => date
            .checked_sub_months(Months::new(periods * 3))
            .unwrap_or(date),
        _ => date - Duration::days(periods as i64),
    }
}

/// Shift a timestamp back by `periods` units of `granularity`.
pub fn shift_datetime_back(
    dt: DateTime<FixedOffset>,
    granularity: &str,
    periods: u32,
) -> DateTime<FixedOffset> {
    match granularity {
        "week" => dt - Duration::days(7 * periods as i64),
        "month" => dt.checked_sub_months(Months::new(periods)).unwrap_or(dt),
        "quarter" => dt
            .checked_sub_months(Months::new(periods * 3))
            .unwrap_or(dt),
        _ => dt - Duration::days(periods as i64),
    }
}

/// Same-phase comparison bucket one seasonal cycle before `[start, end)`.
///
/// Both boundaries shift back by the same seasonal offset, so the returned
/// window keeps the anomaly bucket's width and phase. `end` is made inclusive
/// (minus one second) to match the airlayer explain range convention used by
/// the analytics tool caller.
pub fn previous_seasonal_range(
    start: DateTime<FixedOffset>,
    end: DateTime<FixedOffset>,
    granularity: &str,
    seasonal_period: Option<i32>,
) -> (DateTime<FixedOffset>, DateTime<FixedOffset>) {
    let periods = resolve_seasonal_period(seasonal_period, granularity);
    let prev_start = shift_datetime_back(start, granularity, periods);
    let prev_end = shift_datetime_back(end, granularity, periods) - Duration::seconds(1);
    (prev_start, prev_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn dt(s: &str) -> DateTime<FixedOffset> {
        s.parse().unwrap()
    }

    #[test]
    fn default_period_matches_detector_defaults() {
        assert_eq!(default_seasonal_period("day"), 7);
        assert_eq!(default_seasonal_period("week"), 4);
        assert_eq!(default_seasonal_period("month"), 12);
        assert_eq!(default_seasonal_period("quarter"), 4);
        assert_eq!(default_seasonal_period("hour"), 7);
    }

    #[test]
    fn resolve_prefers_persisted_positive_value() {
        assert_eq!(resolve_seasonal_period(Some(30), "day"), 30);
        // Null / non-positive fall back to the granularity default.
        assert_eq!(resolve_seasonal_period(None, "day"), 7);
        assert_eq!(resolve_seasonal_period(Some(0), "week"), 4);
        assert_eq!(resolve_seasonal_period(Some(-1), "month"), 12);
    }

    #[test]
    fn daily_shifts_to_same_weekday_not_the_day_before() {
        // 2026-03-16 is a Monday. A weekly (7) cycle lands on the prior Monday,
        // not Sunday the 15th — the crux of the "compared to weekend" bug.
        let mon = date("2026-03-16");
        let prev = shift_date_back(mon, "day", 7);
        assert_eq!(prev, date("2026-03-09"));
        assert_eq!(prev.format("%A").to_string(), "Monday");
    }

    #[test]
    fn daily_honors_custom_seasonality() {
        // A daily monitor overridden to seasonality 30 compares 30 days back.
        let d = date("2026-03-31");
        assert_eq!(shift_date_back(d, "day", 30), date("2026-03-01"));
    }

    #[test]
    fn weekly_and_monthly_use_calendar_math() {
        assert_eq!(
            shift_date_back(date("2026-03-16"), "week", 4),
            date("2026-02-16")
        );
        // 12-month cycle → same day-of-month a year earlier.
        assert_eq!(
            shift_date_back(date("2026-03-15"), "month", 12),
            date("2025-03-15")
        );
        assert_eq!(
            shift_date_back(date("2026-05-10"), "quarter", 1),
            date("2026-02-10")
        );
    }

    #[test]
    fn previous_range_preserves_width_and_phase() {
        // Daily bucket [Mon 00:00, Tue 00:00) with weekly seasonality → the
        // prior Monday's full day.
        let start = dt("2026-03-16T00:00:00+00:00");
        let end = dt("2026-03-17T00:00:00+00:00");
        let (ps, pe) = previous_seasonal_range(start, end, "day", Some(7));
        assert_eq!(ps, dt("2026-03-09T00:00:00+00:00"));
        assert_eq!(pe, dt("2026-03-09T23:59:59+00:00"));
    }

    #[test]
    fn previous_range_falls_back_when_unset() {
        let start = dt("2026-03-16T00:00:00+00:00");
        let end = dt("2026-03-17T00:00:00+00:00");
        let (ps, _) = previous_seasonal_range(start, end, "day", None);
        // Default weekly cycle, still same weekday.
        assert_eq!(ps, dt("2026-03-09T00:00:00+00:00"));
    }
}
