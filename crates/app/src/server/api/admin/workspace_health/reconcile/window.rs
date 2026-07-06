//! Pure window resolution: `Window` + reference instant → inclusive
//! `[start, end]` date strings for airlayer's `TimeDimensionQuery.date_range`.

use chrono::{Datelike, Duration, Months, NaiveDate, Weekday};

use super::{Grain, WeekStart, Window};

/// Resolve to an inclusive `[start, end]` (`%Y-%m-%d`) for airlayer's
/// `TimeDimensionQuery.date_range` and Toast's business-date window. All grains
/// snap to **calendar** boundaries so the two operand sides compare the same
/// range: `day` is literal days; `week` snaps to `week_start`-aligned weeks;
/// `month` snaps to calendar months. `offset` shifts the whole window back by
/// `offset` grains so the incomplete current period is excluded (`offset: 1,
/// grain: day` == "yesterday"; `offset: 1, grain: week` == "last full week").
pub fn resolve_window(w: &Window, now: chrono::DateTime<chrono::Utc>) -> [String; 2] {
    let today = now.date_naive();
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
    [fmt(start), fmt(end)]
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
            week_start: WeekStart::Sunday,
        }
    }

    #[test]
    fn yesterday_single_day() {
        // last:1 day, offset:1, ref = 2026-06-24 → just 2026-06-23.
        assert_eq!(
            resolve_window(&win(1, Grain::Day, 1), at(2026, 6, 24)),
            ["2026-06-23".to_string(), "2026-06-23".to_string()]
        );
    }

    #[test]
    fn last_7_days_excluding_today() {
        // last:7 days, offset:1, ref = 2026-06-24 → 2026-06-17..=2026-06-23.
        assert_eq!(
            resolve_window(&win(7, Grain::Day, 1), at(2026, 6, 24)),
            ["2026-06-17".to_string(), "2026-06-23".to_string()]
        );
    }

    #[test]
    fn last_full_week_sunday_start() {
        // ref = Wed 2026-06-24. Current week (Sun start) began Sun 2026-06-21;
        // last full week = Sun 2026-06-14 .. Sat 2026-06-20.
        assert_eq!(
            resolve_window(&win(1, Grain::Week, 1), at(2026, 6, 24)),
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
            resolve_window(&w, at(2026, 6, 24)),
            ["2026-06-15".to_string(), "2026-06-21".to_string()]
        );
    }

    #[test]
    fn last_4_weeks_sunday_start() {
        // last:4 weeks, offset:1 → the 4 full weeks before the current one:
        // Sun 2026-05-24 .. Sat 2026-06-20.
        assert_eq!(
            resolve_window(&win(4, Grain::Week, 1), at(2026, 6, 24)),
            ["2026-05-24".to_string(), "2026-06-20".to_string()]
        );
    }

    #[test]
    fn week_anchor_on_start_day() {
        // ref = Sun 2026-06-21 is itself a week start; last full week is the
        // prior Sun..Sat, not the day itself.
        assert_eq!(
            resolve_window(&win(1, Grain::Week, 1), at(2026, 6, 21)),
            ["2026-06-14".to_string(), "2026-06-20".to_string()]
        );
    }

    #[test]
    fn last_full_month() {
        // ref = 2026-06-24, offset:1 → all of May 2026.
        assert_eq!(
            resolve_window(&win(1, Grain::Month, 1), at(2026, 6, 24)),
            ["2026-05-01".to_string(), "2026-05-31".to_string()]
        );
    }

    #[test]
    fn last_full_month_year_wrap() {
        // ref = 2026-01-15, offset:1 → all of December 2025.
        assert_eq!(
            resolve_window(&win(1, Grain::Month, 1), at(2026, 1, 15)),
            ["2025-12-01".to_string(), "2025-12-31".to_string()]
        );
    }

    #[test]
    fn last_3_months() {
        // last:3 months, offset:1, ref = 2026-06-24 → Mar 1 .. May 31 2026.
        assert_eq!(
            resolve_window(&win(3, Grain::Month, 1), at(2026, 6, 24)),
            ["2026-03-01".to_string(), "2026-05-31".to_string()]
        );
    }
}
