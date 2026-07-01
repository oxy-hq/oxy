//! Pure window resolution: `Window` + reference instant → inclusive
//! `[start, end]` date strings for airlayer's `TimeDimensionQuery.date_range`.

use chrono::{Datelike, Duration, NaiveDate};

use super::{Grain, Window};

/// Resolve to an inclusive `[start, end]` (`%Y-%m-%d`). Day-grained only:
/// week/month grains are rejected upstream (`verdict_for` degrades them) because
/// their fixed 7-/30-day spans don't align with calendar periods. We still map
/// them to a day span here as a defensive fallback, but the runner never reaches
/// this with a non-day grain. `offset` shifts the whole window back by `offset`
/// grains so the incomplete current period is excluded.
pub fn resolve_window(w: &Window, now: chrono::DateTime<chrono::Utc>) -> [String; 2] {
    let today = now.date_naive();
    let days_per_grain = match w.grain {
        Grain::Day => 1i64,
        Grain::Week => 7,
        Grain::Month => 30, // unreachable in the runner; kept as a defensive span
    };
    let end_offset_days = w.offset as i64 * days_per_grain;
    let span_days = w.last.max(1) as i64 * days_per_grain;
    let end: NaiveDate = today - Duration::days(end_offset_days);
    let start: NaiveDate = end - Duration::days(span_days - 1);
    [fmt(start), fmt(end)]
}

fn fmt(d: NaiveDate) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::api::admin::workspace_health::reconcile::{Grain, Window};
    use chrono::TimeZone;

    fn at(y: i32, m: u32, d: u32) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap()
    }

    #[test]
    fn yesterday_single_day() {
        // last:1 day, offset:1, ref = 2026-06-24 → just 2026-06-23.
        let w = Window {
            last: 1,
            grain: Grain::Day,
            offset: 1,
        };
        assert_eq!(
            resolve_window(&w, at(2026, 6, 24)),
            ["2026-06-23".to_string(), "2026-06-23".to_string()]
        );
    }

    #[test]
    fn last_7_days_excluding_today() {
        // last:7 days, offset:1, ref = 2026-06-24 → 2026-06-17..=2026-06-23.
        let w = Window {
            last: 7,
            grain: Grain::Day,
            offset: 1,
        };
        assert_eq!(
            resolve_window(&w, at(2026, 6, 24)),
            ["2026-06-17".to_string(), "2026-06-23".to_string()]
        );
    }
}
