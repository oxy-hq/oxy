//! Cron-expression helpers for the Phase 2 scheduler.
//!
//! Pure (no DB). Two entry points: [`validate_cron`] for the CRUD write
//! path, and [`next_occurrence_after`] for the tick. The scheduler stores
//! the schedule row as the source of truth and only ever needs
//! "next occurrence strictly after T in TZ" — no live ticking library.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use croner::Cron;

/// Validate a cron expression + IANA timezone for the CRUD layer.
/// `Ok(())` means [`next_occurrence_after`] will not fail on them.
pub fn validate_cron(expr: &str, timezone: &str) -> Result<(), String> {
    parse_tz(timezone)?;
    parse_cron(expr)?;
    Ok(())
}

/// Next occurrence **strictly after** `after`, evaluated in `timezone`,
/// returned as UTC.
///
/// "Strictly after" is what makes the misfire policy
/// *run-once-then-resume*: the tick passes `now()`, so however many slots
/// were missed during an outage collapse to the single next future
/// occurrence (this function returns one instant, never a backlog).
pub fn next_occurrence_after(
    expr: &str,
    timezone: &str,
    after: DateTime<Utc>,
) -> Result<DateTime<Utc>, String> {
    let tz = parse_tz(timezone)?;
    let cron = parse_cron(expr)?;
    let after_tz = after.with_timezone(&tz);
    let next = cron
        .find_next_occurrence(&after_tz, false)
        .map_err(|e| format!("no next occurrence for {expr:?} in {timezone:?}: {e}"))?;
    Ok(next.with_timezone(&Utc))
}

/// Count cron occurrences in the half-open range `(after, until]`.
///
/// Used by the scheduler tick to detect catch-up fires after an outage:
/// when `prev_next_run_at` is older than one cadence step, the slots
/// between then and now are silently skipped (run-once-then-resume
/// policy). Surfacing the count lets us stamp `missed_runs` on the
/// schedule so the user can see "you missed N runs while the server
/// was down" instead of a silently late catch-up.
///
/// Capped at `max` to keep this O(N) loop bounded — a five-second cron
/// with a multi-year `after` would otherwise spin for a while. If the
/// real count exceeds the cap, returns `max` and the caller can log
/// "≥ max" rather than the exact figure.
pub fn count_occurrences_between(
    expr: &str,
    timezone: &str,
    after: DateTime<Utc>,
    until: DateTime<Utc>,
    max: usize,
) -> Result<usize, String> {
    if until <= after || max == 0 {
        return Ok(0);
    }
    let tz = parse_tz(timezone)?;
    let cron = parse_cron(expr)?;
    let mut cursor_tz = after.with_timezone(&tz);
    let until_tz = until.with_timezone(&tz);
    let mut count = 0usize;
    while count < max {
        let next = cron
            .find_next_occurrence(&cursor_tz, false)
            .map_err(|e| format!("counting occurrences for {expr:?} in {timezone:?}: {e}"))?;
        if next > until_tz {
            break;
        }
        count += 1;
        cursor_tz = next;
    }
    Ok(count)
}

fn parse_tz(timezone: &str) -> Result<Tz, String> {
    timezone
        .parse::<Tz>()
        .map_err(|_| format!("invalid timezone {timezone:?}"))
}

fn parse_cron(expr: &str) -> Result<Cron, String> {
    Cron::from_str(expr).map_err(|e| format!("invalid cron expression {expr:?}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn utc(y: i32, m: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, mi, 0).unwrap()
    }

    #[test]
    fn daily_utc_next_occurrence() {
        // 09:00 every day, UTC. From 08:00 → same-day 09:00.
        let next = next_occurrence_after("0 9 * * *", "UTC", utc(2026, 5, 19, 8, 0)).unwrap();
        assert_eq!(next, utc(2026, 5, 19, 9, 0));
    }

    #[test]
    fn strictly_after_skips_exact_match() {
        // From exactly 09:00 → the *next* day's 09:00, not the same instant.
        let next = next_occurrence_after("0 9 * * *", "UTC", utc(2026, 5, 19, 9, 0)).unwrap();
        assert_eq!(next, utc(2026, 5, 20, 9, 0));
    }

    #[test]
    fn timezone_is_respected() {
        // 09:00 America/New_York. On 2026-05-19 NY is EDT (UTC-4), so
        // 09:00 local == 13:00 UTC.
        let next =
            next_occurrence_after("0 9 * * *", "America/New_York", utc(2026, 5, 19, 0, 0)).unwrap();
        assert_eq!(next, utc(2026, 5, 19, 13, 0));
    }

    #[test]
    fn misfire_collapses_to_single_future_slot() {
        // Daily schedule, "after" is a week in the past → returns the one
        // next future occurrence, never a backlog of missed slots.
        let after = utc(2026, 5, 12, 10, 0);
        let next = next_occurrence_after("0 9 * * *", "UTC", after).unwrap();
        assert_eq!(next, utc(2026, 5, 13, 9, 0));
        assert!(next > after);
    }

    #[test]
    fn invalid_inputs_error() {
        assert!(next_occurrence_after("not a cron", "UTC", utc(2026, 5, 19, 0, 0)).is_err());
        assert!(
            next_occurrence_after("0 9 * * *", "Mars/Olympus", utc(2026, 5, 19, 0, 0)).is_err()
        );
        assert!(validate_cron("0 9 * * *", "UTC").is_ok());
        assert!(validate_cron("bogus", "UTC").is_err());
        assert!(validate_cron("0 9 * * *", "Nowhere/Nope").is_err());
    }

    // ── §12 FU3: cron edge coverage ──────────────────────────────────────

    #[test]
    fn step_expression_spacing() {
        // Every 15 minutes → consecutive occurrences are 15 min apart.
        let a = next_occurrence_after("*/15 * * * *", "UTC", utc(2026, 5, 19, 9, 7)).unwrap();
        assert_eq!(a, utc(2026, 5, 19, 9, 15));
        let b = next_occurrence_after("*/15 * * * *", "UTC", a).unwrap();
        assert_eq!(b, utc(2026, 5, 19, 9, 30));
    }

    #[test]
    fn leap_day_is_deterministic() {
        // Feb 29 only exists on leap years; from 2026 the next is 2028.
        let next = next_occurrence_after("0 0 29 2 *", "UTC", utc(2026, 3, 1, 0, 0)).unwrap();
        assert_eq!(next, utc(2028, 2, 29, 0, 0));
    }

    #[test]
    fn dom_dow_is_or_semantics() {
        // Standard cron: when BOTH day-of-month and day-of-week are
        // restricted, they OR. "0 0 13 * 5" = midnight on the 13th OR any
        // Friday. From Tue 2026-05-19, the next Friday (2026-05-22) comes
        // before the 13th — documents croner's OR behavior.
        let next = next_occurrence_after("0 0 13 * 5", "UTC", utc(2026, 5, 19, 0, 0)).unwrap();
        assert_eq!(next, utc(2026, 5, 22, 0, 0));
    }

    #[test]
    fn dst_spring_forward_still_resolves() {
        // 2026-03-08 America/New_York: 02:00→03:00 (02:30 doesn't exist).
        // A 02:30 daily schedule must still yield a valid future instant
        // (croner adjusts; we only assert monotonic + Ok, not the exact
        // wall-clock, since that is impl-defined for the gap).
        let after = utc(2026, 3, 8, 0, 0);
        let next = next_occurrence_after("30 2 * * *", "America/New_York", after).unwrap();
        assert!(next > after);
    }

    #[test]
    fn dst_fall_back_still_resolves() {
        // 2026-11-01 America/New_York: 02:00→01:00 (01:30 occurs twice).
        // A 01:30 daily schedule must resolve to a single valid instant.
        let after = utc(2026, 11, 1, 0, 0);
        let next = next_occurrence_after("30 1 * * *", "America/New_York", after).unwrap();
        assert!(next > after);
    }

    #[test]
    fn six_field_seconds_behavior_is_documented() {
        // croner's default `Cron::from_str` is 5-field. A 6-field (with
        // seconds) string is exercised here so the behavior is pinned by
        // a test rather than assumed: whatever it does (parse or reject)
        // must at least be deterministic and not panic.
        let r = next_occurrence_after("0 0 9 * * *", "UTC", utc(2026, 5, 19, 0, 0));
        let r2 = next_occurrence_after("0 0 9 * * *", "UTC", utc(2026, 5, 19, 0, 0));
        assert_eq!(r.is_ok(), r2.is_ok(), "6-field handling must be stable");
        if let Ok(dt) = r {
            assert!(dt > utc(2026, 5, 19, 0, 0));
        }
    }

    // ── count_occurrences_between ───────────────────────────────────────

    #[test]
    fn count_returns_zero_when_until_le_after() {
        // Defensive: until == after and until < after both → 0.
        let after = utc(2026, 5, 19, 9, 0);
        assert_eq!(
            count_occurrences_between("0 9 * * *", "UTC", after, after, 100).unwrap(),
            0
        );
        let earlier = utc(2026, 5, 18, 9, 0);
        assert_eq!(
            count_occurrences_between("0 9 * * *", "UTC", after, earlier, 100).unwrap(),
            0
        );
    }

    #[test]
    fn count_excludes_after_and_includes_until() {
        // Hourly schedule. After = 10:00, until = 14:00.
        // (10:00, 14:00] contains 11:00, 12:00, 13:00, 14:00 = 4 occurrences.
        // The 10:00 itself is excluded (it's the slot we're already firing).
        let after = utc(2026, 5, 19, 10, 0);
        let until = utc(2026, 5, 19, 14, 0);
        let n = count_occurrences_between("0 * * * *", "UTC", after, until, 100).unwrap();
        assert_eq!(n, 4);
    }

    #[test]
    fn count_zero_for_no_missed_slots() {
        // After = 09:00, until = 09:30. Daily schedule. No occurrence in
        // that 30-minute window — catch-up was on time.
        let after = utc(2026, 5, 19, 9, 0);
        let until = utc(2026, 5, 19, 9, 30);
        let n = count_occurrences_between("0 9 * * *", "UTC", after, until, 100).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn count_caps_at_max_for_pathological_gaps() {
        // Every-minute cron. A 10-hour gap → 600 occurrences. Cap at 50
        // and verify we return exactly the cap, not the real figure or
        // a runaway loop.
        let after = utc(2026, 5, 19, 0, 0);
        let until = utc(2026, 5, 19, 10, 0);
        let n = count_occurrences_between("* * * * *", "UTC", after, until, 50).unwrap();
        assert_eq!(n, 50);
    }

    #[test]
    fn count_respects_timezone() {
        // "0 9 * * *" in America/New_York. After = 2026-05-19 13:00 UTC
        // (= 09:00 NY local — the just-fired slot). Until = 2026-05-21
        // 14:00 UTC. Expected NY 09:00 slots in between:
        //   2026-05-20 13:00 UTC, 2026-05-21 13:00 UTC = 2.
        let after = utc(2026, 5, 19, 13, 0);
        let until = utc(2026, 5, 21, 14, 0);
        let n =
            count_occurrences_between("0 9 * * *", "America/New_York", after, until, 100).unwrap();
        assert_eq!(n, 2);
    }
}
