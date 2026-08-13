//! Single source of truth for observability duration windows.
//!
//! Every duration label the UI exposes (`1h`, `24h`, `7d`, `30d`, `90d`,
//! `all`) is defined here with both the number-of-hours equivalent (used by
//! the retention cleanup) and backend-specific interval fragments (used by
//! the query filters). Retention is derived from the longest finite window —
//! there is no separate `OXY_OBSERVABILITY_RETENTION_DAYS` env var.
//!
//! Adding a new duration is a single edit here; every backend and the
//! retention loop pick it up automatically.
//!
//! Note: `"all"` disables the query filter but does NOT disable retention —
//! retention is derived from the longest _finite_ window so `all` will still
//! return older data that hasn't yet been purged but that data is considered
//! out-of-SLA and may be removed at any time.

/// One entry in the supported-duration set.
///
/// `label` is the string the frontend sends (e.g. `"7d"`, `"all"`).
/// `hours` is `None` for the unbounded `"all"` window.
/// `clickhouse_interval` is the literal fragment ClickHouse accepts
/// (includes the `INTERVAL` keyword).
pub struct DurationWindow {
    pub label: &'static str,
    pub hours: Option<u32>,
    pub clickhouse_interval: Option<&'static str>,
}

/// All duration windows the UI can request. Ordered from shortest to longest;
/// the retention loop uses the last finite entry.
pub const DURATIONS: &[DurationWindow] = &[
    DurationWindow {
        label: "1h",
        hours: Some(1),
        clickhouse_interval: Some("INTERVAL 1 HOUR"),
    },
    DurationWindow {
        label: "24h",
        hours: Some(24),
        clickhouse_interval: Some("INTERVAL 24 HOUR"),
    },
    DurationWindow {
        label: "7d",
        hours: Some(7 * 24),
        clickhouse_interval: Some("INTERVAL 7 DAY"),
    },
    DurationWindow {
        label: "30d",
        hours: Some(30 * 24),
        clickhouse_interval: Some("INTERVAL 30 DAY"),
    },
    DurationWindow {
        label: "90d",
        hours: Some(90 * 24),
        clickhouse_interval: Some("INTERVAL 90 DAY"),
    },
    DurationWindow {
        label: "all",
        hours: None,
        clickhouse_interval: None,
    },
];

/// Retention derived from the longest finite duration window. This is the
/// single source of truth for how long observability data is kept.
pub const RETENTION_DAYS: u32 = {
    let mut max_hours: u32 = 0;
    let mut i = 0;
    while i < DURATIONS.len() {
        if let Some(h) = DURATIONS[i].hours
            && h > max_hours
        {
            max_hours = h;
        }
        i += 1;
    }
    max_hours / 24
};

fn lookup(label: Option<&str>) -> Option<&'static DurationWindow> {
    let label = label?;
    DURATIONS.iter().find(|d| d.label == label)
}

/// Resolve the ClickHouse interval fragment for a duration label. Returns
/// `None` when the label is unknown or the window is `"all"` (no filter).
/// The fragment includes the `INTERVAL` keyword.
pub fn clickhouse_interval(label: Option<&str>) -> Option<&'static str> {
    lookup(label).and_then(|d| d.clickhouse_interval)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_derived_from_longest_finite_window() {
        assert_eq!(RETENTION_DAYS, 90);
    }

    #[test]
    fn all_returns_none() {
        assert!(clickhouse_interval(Some("all")).is_none());
    }

    #[test]
    fn unknown_returns_none() {
        assert!(clickhouse_interval(Some("bogus")).is_none());
        assert!(clickhouse_interval(None).is_none());
    }

    #[test]
    fn known_durations_resolve() {
        assert_eq!(clickhouse_interval(Some("7d")), Some("INTERVAL 7 DAY"));
        assert_eq!(clickhouse_interval(Some("1h")), Some("INTERVAL 1 HOUR"));
    }
}
