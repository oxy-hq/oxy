//! DuckDB `Value` → `CellValue` / `TypedValue` conversion helpers.

use duckdb::types::{TimeUnit, Value};

use agentic_core::result::{CellValue, TypedDataType, TypedValue};

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

/// Parse a DESCRIBE type string (e.g. `"INTEGER"`, `"DECIMAL(10,2)"`,
/// `"TIMESTAMP_NS"`) into a [`TypedDataType`].
///
/// Unrecognized or parameterised complex types (`LIST`, `STRUCT`, `MAP`,
/// `UNION`) fall through to [`TypedDataType::Unknown`]; callers should then
/// stringify row values for those columns.
pub(super) fn describe_type_to_typed(type_str: &str) -> TypedDataType {
    let up = type_str.to_ascii_uppercase();
    let trimmed = up.trim();

    // DECIMAL(p,s) — parse precision / scale.
    if let Some(rest) = trimmed
        .strip_prefix("DECIMAL(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let mut parts = rest.split(',').map(str::trim);
        let precision = parts
            .next()
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(38);
        let scale = parts.next().and_then(|s| s.parse::<i8>().ok()).unwrap_or(0);
        return TypedDataType::Decimal { precision, scale };
    }

    match trimmed {
        "BOOLEAN" | "BOOL" => TypedDataType::Bool,
        "TINYINT" | "INT1" | "SMALLINT" | "INT2" | "INTEGER" | "INT" | "INT4" | "UTINYINT"
        | "USMALLINT" => TypedDataType::Int32,
        "BIGINT" | "INT8" | "UINTEGER" | "UBIGINT" => TypedDataType::Int64,
        "HUGEINT" | "UHUGEINT" => TypedDataType::Decimal {
            precision: 38,
            scale: 0,
        },
        "FLOAT" | "REAL" | "FLOAT4" | "DOUBLE" | "FLOAT8" => TypedDataType::Float64,
        "VARCHAR" | "CHAR" | "BPCHAR" | "TEXT" | "STRING" | "UUID" => TypedDataType::Text,
        "BLOB" | "BYTEA" | "BINARY" | "VARBINARY" => TypedDataType::Bytes,
        "DATE" => TypedDataType::Date,
        "TIMESTAMP"
        | "TIMESTAMP_S"
        | "TIMESTAMP_MS"
        | "TIMESTAMP_US"
        | "TIMESTAMP_NS"
        | "TIMESTAMPTZ"
        | "TIMESTAMP WITH TIME ZONE"
        | "DATETIME" => TypedDataType::Timestamp,
        "JSON" => TypedDataType::Json,
        _ => TypedDataType::Unknown,
    }
}

/// Convert a `duckdb::types::Value` to a [`TypedValue`], preserving native
/// types wherever [`TypedValue`] has a representation for them.
///
/// The `data_type` hint is used to steer DECIMAL-like HugeInt / UBigInt values
/// and to format Date / Timestamp rendering, but the actual variant chosen is
/// driven by the value itself — callers don't need a perfectly-matching spec.
pub(super) fn duckdb_value_to_typed(v: Value, data_type: &TypedDataType) -> TypedValue {
    match v {
        Value::Null => TypedValue::Null,
        Value::Boolean(b) => TypedValue::Bool(b),
        Value::TinyInt(n) => TypedValue::Int32(n as i32),
        Value::SmallInt(n) => TypedValue::Int32(n as i32),
        Value::Int(n) => TypedValue::Int32(n),
        Value::BigInt(n) => TypedValue::Int64(n),
        Value::UTinyInt(n) => TypedValue::Int32(n as i32),
        Value::USmallInt(n) => TypedValue::Int32(n as i32),
        Value::UInt(n) => TypedValue::Int64(n as i64),
        // u64 → i64 lossy; route through Decimal to preserve the full range.
        Value::UBigInt(n) => match i64::try_from(n) {
            Ok(v) => TypedValue::Int64(v),
            Err(_) => TypedValue::Decimal(n.to_string()),
        },
        // i128 — no direct TypedValue; serialize as Decimal string.
        Value::HugeInt(n) => TypedValue::Decimal(n.to_string()),
        Value::Float(f) => TypedValue::Float64(f as f64),
        Value::Double(f) => TypedValue::Float64(f),
        Value::Decimal(d) => TypedValue::Decimal(d.to_string()),
        Value::Text(s) => TypedValue::Text(s),
        Value::Enum(s) => TypedValue::Text(s),
        Value::Blob(b) => TypedValue::Bytes(b),
        Value::Date32(days) => TypedValue::Date(days),
        Value::Timestamp(unit, value) => {
            let micros = match unit {
                TimeUnit::Second => value.saturating_mul(1_000_000),
                TimeUnit::Millisecond => value.saturating_mul(1_000),
                TimeUnit::Microsecond => value,
                TimeUnit::Nanosecond => value / 1_000,
            };
            TypedValue::Timestamp(micros)
        }
        // TIME-of-day, INTERVAL, and composite types (LIST, STRUCT, MAP, UNION)
        // have no direct TypedValue — emit the driver's string rendering.
        other => {
            let _ = data_type; // hint reserved for future shape-aware rendering
            TypedValue::Text(format!("{other:?}"))
        }
    }
}

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
