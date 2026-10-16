//! Rollup value rendering: unit-level per DuckDB time type, plus both readers end to end.

use std::path::PathBuf;

use agentic_core::result::CellValue;
use duckdb::types::{TimeUnit, ValueRef};
use serde_json::Value;

use super::*;
use crate::compile::PreaggSource;
use crate::preagg::{execute_preagg_sql, execute_preagg_sql_typed};

/// 2026-06-23 00:00:00 UTC — the first row of the live run that showed the bug.
const JUNE_23_SECS: i64 = 1_782_172_800;

fn text(s: &str) -> CellValue {
    CellValue::Text(s.to_string())
}

/// Both renderings of one value, so every case also pins that the typed
/// reader and the JSON reader agree.
fn render(v: ValueRef<'_>) -> (CellValue, Value) {
    (to_cell(v), to_json(v))
}

fn assert_renders(v: ValueRef<'_>, want: &str) {
    assert_eq!(render(v), (text(want), Value::String(want.into())), "{v:?}");
}

#[test]
fn the_reported_microsecond_timestamps_render_as_dates() {
    // `"Timestamp(Microsecond, 1782172800000000)"` reached the agent's rows.
    assert_renders(
        ValueRef::Timestamp(TimeUnit::Microsecond, 1_782_172_800_000_000),
        "2026-06-23",
    );
    assert_renders(
        ValueRef::Timestamp(TimeUnit::Microsecond, 1_782_259_200_000_000),
        "2026-06-24",
    );
}

#[test]
fn every_timestamp_unit_renders_midnight_as_a_bare_date() {
    for (unit, per_second) in [
        (TimeUnit::Second, 1),
        (TimeUnit::Millisecond, 1_000),
        (TimeUnit::Microsecond, 1_000_000),
        (TimeUnit::Nanosecond, 1_000_000_000),
    ] {
        assert_renders(
            ValueRef::Timestamp(unit, JUNE_23_SECS * per_second),
            "2026-06-23",
        );
    }
}

#[test]
fn every_timestamp_unit_renders_a_time_of_day_with_its_fraction() {
    // 13:05:09 on the same day.
    let secs = JUNE_23_SECS + 13 * 3_600 + 5 * 60 + 9;
    assert_renders(
        ValueRef::Timestamp(TimeUnit::Second, secs),
        "2026-06-23T13:05:09",
    );
    assert_renders(
        ValueRef::Timestamp(TimeUnit::Millisecond, secs * 1_000 + 250),
        "2026-06-23T13:05:09.250",
    );
    assert_renders(
        ValueRef::Timestamp(TimeUnit::Microsecond, secs * 1_000_000 + 1),
        "2026-06-23T13:05:09.000001",
    );
    assert_renders(
        ValueRef::Timestamp(TimeUnit::Nanosecond, secs * 1_000_000_000 + 123_456_789),
        "2026-06-23T13:05:09.123456789",
    );
}

#[test]
fn a_timestamp_before_1970_keeps_its_second_and_fraction() {
    // 1969-07-20 20:17:40.500000001 UTC. Truncating division would read the
    // negative count as 20:17:41 with the fraction of the wrong second.
    assert_renders(
        ValueRef::Timestamp(TimeUnit::Nanosecond, -14_182_939_499_999_999),
        "1969-07-20T20:17:40.500000001",
    );
    assert_renders(
        ValueRef::Timestamp(TimeUnit::Millisecond, -14_182_939_500),
        "1969-07-20T20:17:40.500",
    );
}

#[test]
fn dates_render_as_iso_dates() {
    assert_renders(ValueRef::Date32(20_627), "2026-06-23");
    assert_renders(ValueRef::Date32(0), "1970-01-01");
    assert_renders(ValueRef::Date32(-1), "1969-12-31");
}

#[test]
fn times_of_day_render_as_iso_times() {
    let secs = 13 * 3_600 + 5 * 60 + 9;
    assert_renders(ValueRef::Time64(TimeUnit::Second, secs), "13:05:09");
    assert_renders(
        ValueRef::Time64(TimeUnit::Microsecond, secs * 1_000_000 + 250_000),
        "13:05:09.250",
    );
    assert_renders(
        ValueRef::Time64(TimeUnit::Nanosecond, secs * 1_000_000_000 + 1),
        "13:05:09.000000001",
    );
}

#[test]
fn null_stays_null() {
    assert_eq!(render(ValueRef::Null), (CellValue::Null, Value::Null));
}

#[test]
fn midnight_renders_as_a_bare_date() {
    // A coarsened read (`date_trunc` → TIMESTAMP at midnight) must produce
    // the same key shape as a stored-grain read (raw `YYYY-MM-DD` VARCHAR),
    // or a bundle's x-axis silently changes when a rollup's grain does.
    let dt = chrono::NaiveDate::from_ymd_opt(2026, 8, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    assert_eq!(render_naive_datetime(dt), "2026-08-01");
}

#[test]
fn a_time_of_day_renders_iso_with_a_t_separator() {
    let dt = chrono::NaiveDate::from_ymd_opt(2026, 8, 1)
        .unwrap()
        .and_hms_opt(13, 5, 9)
        .unwrap();
    // NaiveDateTime's own Display would emit a space here, which is not
    // ISO-8601 and which some browsers parse as local time.
    assert_eq!(render_naive_datetime(dt), "2026-08-01T13:05:09");

    let sub = chrono::NaiveDate::from_ymd_opt(2026, 8, 1)
        .unwrap()
        .and_hms_milli_opt(13, 5, 9, 250)
        .unwrap();
    assert_eq!(render_naive_datetime(sub), "2026-08-01T13:05:09.250");
}

#[test]
fn date32_days_are_epoch_relative() {
    // 20_301 days after 1970-01-01. The previous shape multiplied by
    // 86_400 and went through a timestamp constructor, which is easy to
    // misread as "this value is seconds".
    assert_eq!(
        date_from_epoch_days(20_301).unwrap().to_string(),
        "2025-08-01"
    );
    // And the epoch itself, plus a pre-epoch day, stay sane.
    assert_eq!(date_from_epoch_days(0).unwrap().to_string(), "1970-01-01");
    assert_eq!(date_from_epoch_days(-1).unwrap().to_string(), "1969-12-31");
}

/// The reader the analytics agent uses, over a real Parquet rollup, with a
/// column of each DuckDB time type. The unit tests above cover the match; this
/// covers that the reader goes through it.
#[test]
fn both_rollup_readers_render_time_columns_iso() {
    let dir = tempfile::tempdir().unwrap();
    let path: PathBuf = dir.path().join("funnel_by_day.parquet");
    let quoted = path.display().to_string().replace('\'', "''");
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!(
        "COPY (SELECT TIMESTAMP '2026-06-23 00:00:00' AS event_date__day, \
         125::BIGINT AS total_paid_signups) TO '{quoted}' (FORMAT PARQUET)"
    ))
    .unwrap();

    let sql = format!(
        "SELECT event_date__day, \
           CAST(event_date__day AS TIMESTAMP_S) AS s, \
           CAST(TIMESTAMP '2026-06-23 13:05:09.25' AS TIMESTAMP_MS) AS ms, \
           TIMESTAMP '2026-06-23 13:05:09.000001' AS us, \
           CAST('2026-06-23 13:05:09.123456789' AS TIMESTAMP_NS) AS ns, \
           CAST(event_date__day AS DATE) AS d, \
           TIME '13:05:09' AS t, \
           CAST(NULL AS TIMESTAMP) AS missing, \
           total_paid_signups \
         FROM read_parquet('{quoted}')"
    );
    let source = PreaggSource::Local(path.clone());
    let want = [
        "2026-06-23",
        "2026-06-23",
        "2026-06-23T13:05:09.250",
        "2026-06-23T13:05:09.000001",
        "2026-06-23T13:05:09.123456789",
        "2026-06-23",
        "13:05:09",
    ];

    let (_, rows, _) = execute_preagg_sql_typed(&sql, &source, 10).unwrap();
    let mut want_cells: Vec<CellValue> = want.iter().map(|s| text(s)).collect();
    want_cells.extend([CellValue::Null, CellValue::Number(125.0)]);
    assert_eq!(rows[0].0, want_cells);

    let json = execute_preagg_sql(&sql, &source).unwrap();
    let row = &json["rows"][0];
    for (col, want) in ["event_date__day", "s", "ms", "us", "ns", "d", "t"]
        .iter()
        .zip(want)
    {
        assert_eq!(row[*col], Value::String(want.into()), "column {col}");
    }
    assert_eq!(row["missing"], Value::Null);
}
