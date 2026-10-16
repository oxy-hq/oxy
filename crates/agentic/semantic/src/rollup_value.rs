//! DuckDB value → cell / JSON conversion for rollup reads.
//!
//! Both rollup readers convert through here: the typed one
//! (`execute_preagg_sql_typed`, which the analytics agent reads) and the JSON
//! one (`execute_preagg_sql`, for apps and the metric tree). They used to keep
//! a match each, and only the JSON one had learned dates and timestamps, so the
//! agent got `"Timestamp(Microsecond, 1782172800000000)"`: in its rows, in
//! its charts, and in what the model read. One module, one rendering.

use agentic_core::result::CellValue;
use duckdb::types::{TimeUnit, ValueRef};
use serde_json::Value;

/// A rollup cell as the analytics agent's [`CellValue`].
pub(crate) fn to_cell(v: ValueRef<'_>) -> CellValue {
    match v {
        ValueRef::Null => CellValue::Null,
        ValueRef::TinyInt(n) => CellValue::Number(n as f64),
        ValueRef::SmallInt(n) => CellValue::Number(n as f64),
        ValueRef::Int(n) => CellValue::Number(n as f64),
        ValueRef::BigInt(n) => CellValue::Number(n as f64),
        ValueRef::UTinyInt(n) => CellValue::Number(n as f64),
        ValueRef::USmallInt(n) => CellValue::Number(n as f64),
        ValueRef::UInt(n) => CellValue::Number(n as f64),
        ValueRef::UBigInt(n) => CellValue::Number(n as f64),
        ValueRef::HugeInt(n) => i64::try_from(n)
            .map(|v| CellValue::Number(v as f64))
            .unwrap_or_else(|_| CellValue::Text(n.to_string())),
        ValueRef::Float(f) => CellValue::Number(f as f64),
        ValueRef::Double(f) => CellValue::Number(f),
        ValueRef::Decimal(d) => {
            let s = d.to_string();
            s.parse::<f64>()
                .map(CellValue::Number)
                .unwrap_or(CellValue::Text(s))
        }
        ValueRef::Text(s) => CellValue::Text(String::from_utf8_lossy(s).into_owned()),
        ValueRef::Blob(b) => CellValue::Text(format!("<blob {} bytes>", b.len())),
        ValueRef::Boolean(b) => CellValue::Text(b.to_string()),
        ValueRef::Date32(days) => CellValue::Text(render_date32(days)),
        ValueRef::Timestamp(unit, ticks) => CellValue::Text(render_timestamp(unit, ticks)),
        ValueRef::Time64(unit, ticks) => CellValue::Text(render_time64(unit, ticks)),
        other => CellValue::Text(format!("{other:?}")),
    }
}

/// A rollup cell as JSON.
pub(crate) fn to_json(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::TinyInt(n) => serde_json::json!(n),
        ValueRef::SmallInt(n) => serde_json::json!(n),
        ValueRef::Int(n) => serde_json::json!(n),
        ValueRef::BigInt(n) => serde_json::json!(n),
        ValueRef::UTinyInt(n) => serde_json::json!(n),
        ValueRef::USmallInt(n) => serde_json::json!(n),
        ValueRef::UInt(n) => serde_json::json!(n),
        ValueRef::UBigInt(n) => serde_json::json!(n),
        // SUM(int32) over a Parquet rollup comes back as HugeInt (i128) even
        // when the value fits easily in i64 — typical for count/sum rollups.
        // Emit as a JSON number when it fits so chart code that expects
        // numeric y-axis values works; fall back to string only for the
        // genuine overflow case.
        ValueRef::HugeInt(n) => i64::try_from(n)
            .map(|v| serde_json::json!(v))
            .unwrap_or_else(|_| Value::String(n.to_string())),
        ValueRef::Float(f) => serde_json::json!(f),
        ValueRef::Double(f) => serde_json::json!(f),
        // Decimal columns (Parquet `decimal(p,s)`) used to fall through the
        // catch-all and serialise as `"Decimal { ... }"` debug strings,
        // breaking every chart whose measure was a SUM/AVG over a money
        // column. Convert to f64 (good enough for chart rendering) and
        // string-fallback only when conversion fails.
        ValueRef::Decimal(d) => {
            let s = d.to_string();
            s.parse::<f64>()
                .ok()
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number)
                .unwrap_or(Value::String(s))
        }
        ValueRef::Text(s) => Value::String(String::from_utf8_lossy(s).into_owned()),
        ValueRef::Blob(b) => Value::String(format!("<blob {} bytes>", b.len())),
        ValueRef::Boolean(b) => serde_json::json!(b),
        // Time values, for the same reason `Decimal` is handled above: the
        // catch-all debug-formats them ("Date32(20148)").
        ValueRef::Date32(days) => Value::String(render_date32(days)),
        ValueRef::Timestamp(unit, ticks) => Value::String(render_timestamp(unit, ticks)),
        ValueRef::Time64(unit, ticks) => Value::String(render_time64(unit, ticks)),
        other => Value::String(format!("{other:?}")),
    }
}

/// A DuckDB `DATE` (days since the Unix epoch) as ISO 8601 `YYYY-MM-DD`.
fn render_date32(days: i32) -> String {
    date_from_epoch_days(days as i64)
        .map(|d| d.to_string())
        .unwrap_or_else(|| days.to_string())
}

/// A DuckDB `TIMESTAMP` in any unit as ISO 8601, via [`render_naive_datetime`].
///
/// `TIMESTAMPTZ` arrives here too, as a UTC instant, and renders in UTC.
fn render_timestamp(unit: TimeUnit, ticks: i64) -> String {
    let (secs, nanos) = split_ticks(unit, ticks);
    chrono::DateTime::from_timestamp(secs, nanos)
        .map(|d| render_naive_datetime(d.naive_utc()))
        .unwrap_or_else(|| ticks.to_string())
}

/// A DuckDB `TIME` (ticks since midnight) as ISO 8601 `HH:MM:SS[.fff]`.
fn render_time64(unit: TimeUnit, ticks: i64) -> String {
    let (secs, nanos) = split_ticks(unit, ticks);
    u32::try_from(secs)
        .ok()
        .and_then(|secs| chrono::NaiveTime::from_num_seconds_from_midnight_opt(secs, nanos))
        .map(|t| t.format("%H:%M:%S%.f").to_string())
        .unwrap_or_else(|| ticks.to_string())
}

/// Whole seconds and the nanoseconds left over. Floor division, not `/`: a
/// pre-1970 instant is a negative count, and truncating toward zero would
/// take the fraction from the wrong second.
fn split_ticks(unit: TimeUnit, ticks: i64) -> (i64, u32) {
    let per_second: i64 = match unit {
        TimeUnit::Second => 1,
        TimeUnit::Millisecond => 1_000,
        TimeUnit::Microsecond => 1_000_000,
        TimeUnit::Nanosecond => 1_000_000_000,
    };
    let nanos_per_tick = 1_000_000_000 / per_second;
    let nanos = ticks.rem_euclid(per_second) * nanos_per_tick;
    (ticks.div_euclid(per_second), nanos as u32)
}

/// DuckDB's `DATE` is a day count from the Unix epoch, not a timestamp.
///
/// Spelled out rather than routed through `DateTime::from_timestamp(days *
/// 86_400, 0)`, which reads as if the input were seconds.
fn date_from_epoch_days(days: i64) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::from_ymd_opt(1970, 1, 1)?.checked_add_signed(chrono::TimeDelta::days(days))
}

/// Canonical rendering for a time value coming out of a rollup read.
///
/// Coarsening a day-grain rollup to week/month makes DuckDB's `date_trunc`
/// return a TIMESTAMP at midnight, while a read at the stored grain projects
/// the stored VARCHAR. Whether a request is coarsened is decided by the rollup
/// manifest and is invisible to the caller, so those two must not render
/// differently — an exactly-midnight instant renders as a bare `YYYY-MM-DD`
/// date. Anything with a time-of-day renders RFC3339-style with a `T`
/// separator (`NaiveDateTime`'s own `Display` uses a space, which is not
/// ISO-8601).
///
/// **The parity guarantee is for a `type: date` time dimension**, whose stored
/// VARCHAR already is `YYYY-MM-DD`. A `type: datetime` dimension stores
/// whatever the source warehouse's JSON emitted for the truncated timestamp
/// (ClickHouse `2026-08-01 00:00:00`, Snowflake `2026-08-01T00:00:00.000`),
/// and that string passes through `ValueRef::Text` untouched at the stored
/// grain — so a coarsened read of the same rollup still renders it differently.
/// Normalising the `Text` passthrough for the time column would close that,
/// and needs the stored grain's column type, which this function does not see.
fn render_naive_datetime(dt: chrono::NaiveDateTime) -> String {
    use chrono::Timelike;
    if dt.time() == chrono::NaiveTime::MIN {
        dt.date().to_string()
    } else if dt.nanosecond() == 0 {
        dt.format("%Y-%m-%dT%H:%M:%S").to_string()
    } else {
        dt.format("%Y-%m-%dT%H:%M:%S%.f").to_string()
    }
}

#[cfg(test)]
#[path = "rollup_value_tests.rs"]
mod tests;
