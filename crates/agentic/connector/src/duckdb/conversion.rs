//! DuckDB `Value` → `CellValue` conversion helpers.

use duckdb::types::{TimeUnit, Value};

use agentic_core::result::CellValue;

/// Convert days since Unix epoch (1970-01-01) to an ISO date string (YYYY-MM-DD).
pub(super) fn epoch_days_to_iso(days: i32) -> String {
    // Algorithm: https://howardhinnant.github.io/date_algorithms.html (civil_from_days)
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe as i64 + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Convert a timestamp (in the given unit, since Unix epoch) to an ISO datetime string.
pub(super) fn epoch_ts_to_iso(unit: &TimeUnit, value: i64) -> String {
    let secs = match unit {
        TimeUnit::Second => value,
        TimeUnit::Millisecond => value / 1_000,
        TimeUnit::Microsecond => value / 1_000_000,
        TimeUnit::Nanosecond => value / 1_000_000_000,
    };
    let sub_secs = match unit {
        TimeUnit::Second => 0i64,
        TimeUnit::Millisecond => (value % 1_000).abs(),
        TimeUnit::Microsecond => (value % 1_000_000).abs(),
        TimeUnit::Nanosecond => (value % 1_000_000_000).abs(),
    };
    let days = secs.div_euclid(86_400) as i32;
    let time_secs = secs.rem_euclid(86_400);
    let h = time_secs / 3600;
    let m = (time_secs % 3600) / 60;
    let s = time_secs % 60;
    let date = epoch_days_to_iso(days);
    if sub_secs == 0 && h == 0 && m == 0 && s == 0 {
        date
    } else if sub_secs == 0 {
        format!("{date} {h:02}:{m:02}:{s:02}")
    } else {
        format!("{date} {h:02}:{m:02}:{s:02}.{sub_secs}")
    }
}

/// Map a `duckdb::types::Value` to the connector-neutral [`CellValue`].
pub(super) fn duckdb_to_cell(v: Value) -> CellValue {
    match v {
        Value::Null => CellValue::Null,
        Value::Boolean(b) => CellValue::Number(if b { 1.0 } else { 0.0 }),
        Value::TinyInt(n) => CellValue::Number(n as f64),
        Value::SmallInt(n) => CellValue::Number(n as f64),
        Value::Int(n) => CellValue::Number(n as f64),
        Value::BigInt(n) => CellValue::Number(n as f64),
        Value::HugeInt(n) => CellValue::Number(n as f64),
        Value::UTinyInt(n) => CellValue::Number(n as f64),
        Value::USmallInt(n) => CellValue::Number(n as f64),
        Value::UInt(n) => CellValue::Number(n as f64),
        Value::UBigInt(n) => CellValue::Number(n as f64),
        Value::Float(f) => CellValue::Number(f as f64),
        Value::Double(f) => CellValue::Number(f),
        Value::Text(s) => CellValue::Text(s),
        Value::Enum(s) => CellValue::Text(s),
        Value::Blob(b) => CellValue::Text(format!("<blob {} bytes>", b.len())),
        Value::Date32(days) => CellValue::Text(epoch_days_to_iso(days)),
        Value::Timestamp(unit, value) => CellValue::Text(epoch_ts_to_iso(&unit, value)),
        Value::Time64(unit, value) => {
            let secs = match unit {
                TimeUnit::Second => value,
                TimeUnit::Millisecond => value / 1_000,
                TimeUnit::Microsecond => value / 1_000_000,
                TimeUnit::Nanosecond => value / 1_000_000_000,
            };
            let h = secs / 3600;
            let m = (secs % 3600) / 60;
            let s = secs % 60;
            CellValue::Text(format!("{h:02}:{m:02}:{s:02}"))
        }
        // Complex types — stringify so the LLM can read them.
        other => CellValue::Text(format!("{other:?}")),
    }
}

// ── Connector ─────────────────────────────────────────────────────────────────

/// DuckDB-backed connector for Parquet/CSV and in-process analytics.
///
/// `duckdb::Connection` uses `RefCell` internally and is not `Sync`.
/// Wrapping it in a `Mutex` gives the `Sync` needed by

pub(super) fn duckdb_to_cell_opt(v: Value) -> Option<CellValue> {
    match v {
        Value::Null => None,
        Value::Boolean(b) => Some(CellValue::Number(if b { 1.0 } else { 0.0 })),
        Value::TinyInt(n) => Some(CellValue::Number(n as f64)),
        Value::SmallInt(n) => Some(CellValue::Number(n as f64)),
        Value::Int(n) => Some(CellValue::Number(n as f64)),
        Value::BigInt(n) => Some(CellValue::Number(n as f64)),
        Value::HugeInt(n) => Some(CellValue::Number(n as f64)),
        Value::UTinyInt(n) => Some(CellValue::Number(n as f64)),
        Value::USmallInt(n) => Some(CellValue::Number(n as f64)),
        Value::UInt(n) => Some(CellValue::Number(n as f64)),
        Value::UBigInt(n) => Some(CellValue::Number(n as f64)),
        Value::Float(f) => Some(CellValue::Number(f as f64)),
        Value::Double(f) => Some(CellValue::Number(f)),
        Value::Text(s) => Some(CellValue::Text(s)),
        Value::Enum(s) => Some(CellValue::Text(s)),
        Value::Blob(_) => None,
        other => Some(CellValue::Text(format!("{other:?}"))),
    }
}
