//! Pure window resolution: `Window` + reference instant → an inclusive
//! `[start, end]` on the window's own calendar, plus the timezone that calendar
//! belongs to.

use chrono::{Datelike, Duration, Months, NaiveDate, Weekday};

use super::{Grain, WeekStart, Window};

/// The resolved comparison window, shared by BOTH operands of a check.
///
/// Dates and timezone travel together deliberately. The dates are wall-clock
/// dates *in* `timezone`; an operand handed one without the other would filter a
/// timezone-converted column against an unconverted range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWindow {
    /// Inclusive `[start, end]`, formatted `%Y-%m-%d`.
    pub dates: [String; 2],
    /// IANA name the dates were resolved in; `"UTC"` when unset.
    pub timezone: String,
}

/// Resolve to an inclusive `[start, end]` (`%Y-%m-%d`) for airlayer's
/// `TimeDimensionQuery.date_range` and Toast's business-date window. All grains
/// snap to **calendar** boundaries so the two operand sides compare the same
/// range: `day` is literal days; `week` snaps to `week_start`-aligned weeks;
/// `month` snaps to calendar months. `offset` shifts the whole window back by
/// `offset` grains so the incomplete current period is excluded (`offset: 1,
/// grain: day` == "yesterday"; `offset: 1, grain: week` == "last full week").
///
/// `freshness` moves the *reference instant* back before any of that, so a
/// warehouse that is days behind on ingestion still compares a period that has
/// actually landed. `timezone` decides which calendar the reference date is
/// read on. With both at their defaults (zero, UTC) this is exactly
/// `now.date_naive()` — every stored config keeps its byte-identical window.
pub fn resolve_window(w: &Window, now: chrono::DateTime<chrono::Utc>) -> ResolvedWindow {
    let tz = w.effective_timezone();
    let today = (now.with_timezone(&tz) - w.effective_freshness()).date_naive();
    let last = w.last.max(1) as i64;
    let offset = w.offset as i64;
    let (start, end) = match w.grain {
        Grain::Day => {
            let end = today - Duration::days(offset);
            (end - Duration::days(last - 1), end)
        }
        Grain::Week => {
            // First day of the (incomplete) current week; every window is a run
            // of whole weeks ending `offset` weeks before it.
            let anchor = start_of_week(today, w.week_start);
            let end = anchor - Duration::days(offset * 7 - 6);
            let start = anchor - Duration::days(offset * 7 + (last - 1) * 7);
            (start, end)
        }
        Grain::Month => {
            // First day of the current month; the target month is `offset`
            // months back, the window spans `last` months ending there.
            let anchor = first_of_month(today);
            let target_first = sub_months(anchor, offset as u32);
            let end = add_months(target_first, 1) - Duration::days(1);
            let start = sub_months(anchor, (offset + last - 1) as u32);
            (start, end)
        }
    };
    ResolvedWindow {
        dates: [fmt(start), fmt(end)],
        timezone: tz.name().to_string(),
    }
}

/// First day of the week containing `d`, aligned to `week_start`.
fn start_of_week(d: NaiveDate, week_start: WeekStart) -> NaiveDate {
    let ws = match week_start {
        WeekStart::Sunday => Weekday::Sun,
        WeekStart::Monday => Weekday::Mon,
    };
    let delta =
        (7 + d.weekday().num_days_from_monday() as i64 - ws.num_days_from_monday() as i64) % 7;
    d - Duration::days(delta)
}

fn first_of_month(d: NaiveDate) -> NaiveDate {
    d.with_day(1).expect("day 1 is always valid")
}

/// `first-of-month` date shifted back `n` calendar months (year-wrap safe).
fn sub_months(first_of_month: NaiveDate, n: u32) -> NaiveDate {
    first_of_month
        .checked_sub_months(Months::new(n))
        .expect("month subtraction stays in range")
}

fn add_months(first_of_month: NaiveDate, n: u32) -> NaiveDate {
    first_of_month
        .checked_add_months(Months::new(n))
        .expect("month addition stays in range")
}

fn fmt(d: NaiveDate) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::api::admin::workspace_health::reconcile::{Grain, WeekStart, Window};
    use chrono::TimeZone;

    fn at(y: i32, m: u32, d: u32) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap()
    }

    fn win(last: u32, grain: Grain, offset: u32) -> Window {
        Window {
            last,
            grain,
            offset,
            freshness: None,
            timezone: None,
            week_start: WeekStart::Sunday,
        }
    }

    fn win_fresh(last: u32, grain: Grain, offset: u32, freshness: &str) -> Window {
        let mut w = win(last, grain, offset);
        w.freshness = Some(humantime::parse_duration(freshness).unwrap());
        w
    }

    fn win_tz(last: u32, grain: Grain, offset: u32, tz: &str) -> Window {
        let mut w = win(last, grain, offset);
        w.timezone = Some(tz.to_string());
        w
    }

    /// The pre-change calculation, verbatim, as an independent oracle. Do NOT
    /// refactor this to call anything in `super` — its whole value is that it
    /// is a second implementation, written from the old source.
    fn legacy_utc_window(w: &Window, now: chrono::DateTime<chrono::Utc>) -> [String; 2] {
        let today = now.date_naive();
        let last = w.last.max(1) as i64;
        let offset = w.offset as i64;
        let (start, end) = match w.grain {
            Grain::Day => {
                let end = today - Duration::days(offset);
                (end - Duration::days(last - 1), end)
            }
            Grain::Week => {
                let anchor = start_of_week(today, w.week_start);
                let end = anchor - Duration::days(offset * 7 - 6);
                let start = anchor - Duration::days(offset * 7 + (last - 1) * 7);
                (start, end)
            }
            Grain::Month => {
                let anchor = first_of_month(today);
                let target_first = sub_months(anchor, offset as u32);
                let end = add_months(target_first, 1) - Duration::days(1);
                let start = sub_months(anchor, (offset + last - 1) as u32);
                (start, end)
            }
        };
        [fmt(start), fmt(end)]
    }

    #[test]
    fn absent_timezone_and_freshness_reproduce_the_utc_window_exactly() {
        // The back-compat guard. Every stored config has neither field set, so
        // every one of them must resolve byte-identically to the pre-change
        // calculation — across every grain, offset, week_start and span.
        //
        // Several reference instants, including ones near a UTC midnight and on
        // a month/year boundary, where a mis-taken reference date would show up.
        let instants = [
            at(2026, 6, 24),
            chrono::Utc.with_ymd_and_hms(2026, 6, 24, 0, 0, 0).unwrap(),
            chrono::Utc
                .with_ymd_and_hms(2026, 6, 24, 23, 59, 59)
                .unwrap(),
            at(2026, 1, 1),
            at(2026, 3, 1),
            at(2026, 12, 31),
        ];
        for now in instants {
            for grain in [Grain::Day, Grain::Week, Grain::Month] {
                for week_start in [WeekStart::Sunday, WeekStart::Monday] {
                    for offset in 0..4u32 {
                        for last in 1..4u32 {
                            let mut w = win(last, grain, offset);
                            w.week_start = week_start;
                            assert!(w.timezone.is_none() && w.freshness.is_none());

                            let got = resolve_window(&w, now);
                            assert_eq!(
                                got.dates,
                                legacy_utc_window(&w, now),
                                "{now} {grain:?} {week_start:?} last={last} offset={offset}"
                            );
                            assert_eq!(got.timezone, "UTC");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn freshness_shifts_the_day_window_back_whole_days() {
        // ref = 2026-06-24 12:00Z, freshness 3d => watermark date 2026-06-21,
        // then offset:1 day off that => 2026-06-20.
        assert_eq!(
            resolve_window(&win_fresh(1, Grain::Day, 1, "3d"), at(2026, 6, 24)).dates,
            ["2026-06-20".to_string(), "2026-06-20".to_string()]
        );
    }

    #[test]
    fn freshness_composes_with_offset() {
        // freshness applies FIRST (moving the reference instant), then offset
        // counts grains off the watermark's calendar date.
        // ref 2026-06-24, freshness 2d => 2026-06-22; offset 3 days => 2026-06-19.
        assert_eq!(
            resolve_window(&win_fresh(1, Grain::Day, 3, "2d"), at(2026, 6, 24)).dates,
            ["2026-06-19".to_string(), "2026-06-19".to_string()]
        );
    }

    #[test]
    fn sub_day_freshness_can_cross_a_date_boundary() {
        // 2026-06-24T12:00Z minus 13h = 2026-06-23T23:00Z => watermark date
        // 2026-06-23, so offset:1 day lands on 2026-06-22.
        assert_eq!(
            resolve_window(&win_fresh(1, Grain::Day, 1, "13h"), at(2026, 6, 24)).dates,
            ["2026-06-22".to_string(), "2026-06-22".to_string()]
        );
    }

    #[test]
    fn timezone_moves_the_reference_to_the_local_calendar_date() {
        // 2026-07-28T04:00Z is still 2026-07-27 21:00 in Los Angeles, so the
        // local calendar date is a day behind UTC. offset:1 day => 2026-07-26,
        // whereas the UTC resolution would give 2026-07-27.
        let now = chrono::Utc.with_ymd_and_hms(2026, 7, 28, 4, 0, 0).unwrap();
        let local = resolve_window(&win_tz(1, Grain::Day, 1, "America/Los_Angeles"), now);
        assert_eq!(
            local.dates,
            ["2026-07-26".to_string(), "2026-07-26".to_string()]
        );
        assert_eq!(local.timezone, "America/Los_Angeles");

        // Same instant, no timezone => the UTC calendar, one day later.
        assert_eq!(
            resolve_window(&win(1, Grain::Day, 1), now).dates,
            ["2026-07-27".to_string(), "2026-07-27".to_string()]
        );
    }

    #[test]
    fn freshness_crosses_a_week_boundary() {
        // ref = Wed 2026-06-24, freshness 5d => watermark Fri 2026-06-19, whose
        // Sunday-start week began 2026-06-14. offset:1 => the week before that:
        // Sun 2026-06-07 .. Sat 2026-06-13.
        assert_eq!(
            resolve_window(&win_fresh(1, Grain::Week, 1, "5d"), at(2026, 6, 24)).dates,
            ["2026-06-07".to_string(), "2026-06-13".to_string()]
        );
    }

    #[test]
    fn freshness_crosses_a_month_boundary() {
        // ref = 2026-06-02, freshness 3d => watermark 2026-05-30, so the current
        // month is May and offset:1 => all of April 2026.
        assert_eq!(
            resolve_window(&win_fresh(1, Grain::Month, 1, "3d"), at(2026, 6, 2)).dates,
            ["2026-04-01".to_string(), "2026-04-30".to_string()]
        );
    }

    #[test]
    fn timezone_and_freshness_compose() {
        // 2026-07-28T04:00Z in Los Angeles is 2026-07-27 21:00 local; minus 3d
        // is 2026-07-24 21:00 local => watermark date 2026-07-24; offset:1 day
        // => 2026-07-23.
        let now = chrono::Utc.with_ymd_and_hms(2026, 7, 28, 4, 0, 0).unwrap();
        let w = {
            let mut w = win_tz(1, Grain::Day, 1, "America/Los_Angeles");
            w.freshness = Some(humantime::parse_duration("3d").unwrap());
            w
        };
        assert_eq!(
            resolve_window(&w, now).dates,
            ["2026-07-23".to_string(), "2026-07-23".to_string()]
        );
    }

    #[test]
    fn yesterday_single_day() {
        // last:1 day, offset:1, ref = 2026-06-24 → just 2026-06-23.
        assert_eq!(
            resolve_window(&win(1, Grain::Day, 1), at(2026, 6, 24)).dates,
            ["2026-06-23".to_string(), "2026-06-23".to_string()]
        );
    }

    #[test]
    fn last_7_days_excluding_today() {
        // last:7 days, offset:1, ref = 2026-06-24 → 2026-06-17..=2026-06-23.
        assert_eq!(
            resolve_window(&win(7, Grain::Day, 1), at(2026, 6, 24)).dates,
            ["2026-06-17".to_string(), "2026-06-23".to_string()]
        );
    }

    #[test]
    fn last_full_week_sunday_start() {
        // ref = Wed 2026-06-24. Current week (Sun start) began Sun 2026-06-21;
        // last full week = Sun 2026-06-14 .. Sat 2026-06-20.
        assert_eq!(
            resolve_window(&win(1, Grain::Week, 1), at(2026, 6, 24)).dates,
            ["2026-06-14".to_string(), "2026-06-20".to_string()]
        );
    }

    #[test]
    fn last_full_week_monday_start() {
        // ref = Wed 2026-06-24. Current week (Mon start) began Mon 2026-06-22;
        // last full week = Mon 2026-06-15 .. Sun 2026-06-21.
        let mut w = win(1, Grain::Week, 1);
        w.week_start = WeekStart::Monday;
        assert_eq!(
            resolve_window(&w, at(2026, 6, 24)).dates,
            ["2026-06-15".to_string(), "2026-06-21".to_string()]
        );
    }

    #[test]
    fn last_4_weeks_sunday_start() {
        // last:4 weeks, offset:1 → the 4 full weeks before the current one:
        // Sun 2026-05-24 .. Sat 2026-06-20.
        assert_eq!(
            resolve_window(&win(4, Grain::Week, 1), at(2026, 6, 24)).dates,
            ["2026-05-24".to_string(), "2026-06-20".to_string()]
        );
    }

    #[test]
    fn week_anchor_on_start_day() {
        // ref = Sun 2026-06-21 is itself a week start; last full week is the
        // prior Sun..Sat, not the day itself.
        assert_eq!(
            resolve_window(&win(1, Grain::Week, 1), at(2026, 6, 21)).dates,
            ["2026-06-14".to_string(), "2026-06-20".to_string()]
        );
    }

    #[test]
    fn last_full_month() {
        // ref = 2026-06-24, offset:1 → all of May 2026.
        assert_eq!(
            resolve_window(&win(1, Grain::Month, 1), at(2026, 6, 24)).dates,
            ["2026-05-01".to_string(), "2026-05-31".to_string()]
        );
    }

    #[test]
    fn last_full_month_year_wrap() {
        // ref = 2026-01-15, offset:1 → all of December 2025.
        assert_eq!(
            resolve_window(&win(1, Grain::Month, 1), at(2026, 1, 15)).dates,
            ["2025-12-01".to_string(), "2025-12-31".to_string()]
        );
    }

    #[test]
    fn last_3_months() {
        // last:3 months, offset:1, ref = 2026-06-24 → Mar 1 .. May 31 2026.
        assert_eq!(
            resolve_window(&win(3, Grain::Month, 1), at(2026, 6, 24)).dates,
            ["2026-03-01".to_string(), "2026-05-31".to_string()]
        );
    }
}
