//! Row-oriented typed conversion helpers for the ClickHouse backend.
//!
//! ClickHouse's HTTP `FORMAT JSONCompact` response gives us both column
//! metadata (with CH's rich type strings) and each row as
//! `Vec<serde_json::Value>`. This module translates those into
//! [`TypedDataType`] / [`TypedValue`] for [`execute_query_full`].
//!
//! The type parser understands the wrappers CH sends in column metadata
//! (`Nullable(...)`, `LowCardinality(...)`) plus the common scalar types.
//! Composites (`Array`, `Tuple`, `Map`, `Nested`, etc.) map to
//! [`TypedDataType::Json`] — ClickHouse encodes them as JSON arrays /
//! objects in JSONCompact, so the already-deserialized `Value` threads
//! through unchanged.

use agentic_core::result::{ColumnSpec, TypedDataType, TypedRowError, TypedValue};
use serde_json::Value;

// ── Type mapping: ClickHouse type string → TypedDataType ────────────────────

/// Parse a ClickHouse column type string (as returned by the JSONCompact
/// `meta.type` field or by `system.columns.type`) into a [`TypedDataType`].
pub(crate) fn ch_type_to_typed(type_str: &str) -> TypedDataType {
    let inner = strip_type_wrappers(type_str.trim());

    // Prefix-matched types (parameterised).
    if let Some(rest) = inner.strip_prefix("Decimal") {
        return parse_decimal(rest);
    }
    if inner.starts_with("DateTime64") {
        return TypedDataType::Timestamp;
    }
    if inner.starts_with("DateTime") {
        return TypedDataType::Timestamp;
    }
    if inner.starts_with("FixedString") {
        return TypedDataType::Text;
    }
    if inner.starts_with("Enum") {
        return TypedDataType::Text;
    }
    // Composite types are stringified/JSONified downstream.
    if inner.starts_with("Array")
        || inner.starts_with("Tuple")
        || inner.starts_with("Map")
        || inner.starts_with("Nested")
        || inner.starts_with("AggregateFunction")
        || inner.starts_with("SimpleAggregateFunction")
    {
        return TypedDataType::Json;
    }

    match inner {
        "Bool" | "Boolean" => TypedDataType::Bool,
        "Int8" | "Int16" | "Int32" | "UInt8" | "UInt16" => TypedDataType::Int32,
        // UInt32 fits in i64; Int64 / UInt64 / Int128+ may overflow — fall back
        // to Decimal string in `parse_ch_cell`.
        "Int64" | "UInt32" | "UInt64" => TypedDataType::Int64,
        "Int128" | "UInt128" | "Int256" | "UInt256" => TypedDataType::Decimal {
            precision: 38,
            scale: 0,
        },
        "Float32" | "Float64" => TypedDataType::Float64,
        "String" => TypedDataType::Text,
        "UUID" | "IPv4" | "IPv6" => TypedDataType::Text,
        "Date" | "Date32" => TypedDataType::Date,
        "JSON" | "Object('json')" | "Object(Nullable('json'))" => TypedDataType::Json,
        "Nothing" => TypedDataType::Unknown,
        _ => TypedDataType::Unknown,
    }
}

/// Peel off `Nullable(...)` and `LowCardinality(...)` wrappers. Both are
/// transparent for type mapping — the underlying CH value is still delivered
/// with the inner type's JSONCompact shape.
fn strip_type_wrappers(type_str: &str) -> &str {
    let mut s = type_str;
    loop {
        s = s.trim();
        if let Some(inner) = s
            .strip_prefix("Nullable(")
            .and_then(|v| v.strip_suffix(')'))
        {
            s = inner;
        } else if let Some(inner) = s
            .strip_prefix("LowCardinality(")
            .and_then(|v| v.strip_suffix(')'))
        {
            s = inner;
        } else {
            return s;
        }
    }
}

/// Parse `(p,s)` or `(p)` from `Decimal(18,2)` / `Decimal32(4)` / etc.
fn parse_decimal(rest: &str) -> TypedDataType {
    // Forms we accept:
    //   `Decimal(18, 2)` → caller passes `(18, 2)`
    //   `Decimal32(4)`   → caller passes `32(4)`
    //   `Decimal(38)`    → caller passes `(38)`
    let after_kind = rest.trim_start_matches(|c: char| c.is_ascii_digit());
    let inside = after_kind
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or("");
    let mut parts = inside.split(',').map(str::trim);
    let precision = parts
        .next()
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(38);
    let scale = parts.next().and_then(|s| s.parse::<i8>().ok()).unwrap_or(0);
    TypedDataType::Decimal { precision, scale }
}

// ── JSONCompact cell → TypedValue ────────────────────────────────────────────

/// Decode a single JSONCompact cell value into a [`TypedValue`].
///
/// ClickHouse returns many numerics as strings (Int64, UInt32/64, Decimal,
/// big ints) so every numeric path tolerates both `Value::Number` and
/// `Value::String`. Date / DateTime cells always arrive as strings.
pub(crate) fn parse_ch_cell(value: &Value, col: &ColumnSpec) -> Result<TypedValue, TypedRowError> {
    if value.is_null() {
        return Ok(TypedValue::Null);
    }

    fn mapping_err(
        col: &ColumnSpec,
        value: &Value,
        detail: impl std::fmt::Display,
    ) -> TypedRowError {
        TypedRowError::TypeMappingError {
            column: col.name.clone(),
            native_type: format!("{:?}", col.data_type),
            message: format!("could not decode '{value}': {detail}"),
        }
    }

    match &col.data_type {
        TypedDataType::Bool => match value {
            Value::Bool(b) => Ok(TypedValue::Bool(*b)),
            Value::Number(n) => Ok(TypedValue::Bool(n.as_i64().unwrap_or(0) != 0)),
            Value::String(s) => match s.as_str() {
                "true" | "1" => Ok(TypedValue::Bool(true)),
                "false" | "0" => Ok(TypedValue::Bool(false)),
                _ => Err(mapping_err(col, value, "unrecognised bool literal")),
            },
            _ => Err(mapping_err(col, value, "expected bool")),
        },
        TypedDataType::Int32 => number_as_i64(value)
            .and_then(|n| i32::try_from(n).ok())
            .map(TypedValue::Int32)
            .ok_or_else(|| mapping_err(col, value, "not a 32-bit integer")),
        TypedDataType::Int64 => number_as_i64(value)
            .map(TypedValue::Int64)
            .or_else(|| {
                // UInt64 that overflows i64 arrives as a string like "18446744073709551615".
                value.as_str().map(|s| TypedValue::Decimal(s.to_string()))
            })
            .ok_or_else(|| mapping_err(col, value, "not a 64-bit integer")),
        TypedDataType::Float64 => number_as_f64(value)
            .map(TypedValue::Float64)
            .ok_or_else(|| mapping_err(col, value, "not a number")),
        TypedDataType::Text => match value {
            Value::String(s) => Ok(TypedValue::Text(s.clone())),
            other => Ok(TypedValue::Text(other.to_string())),
        },
        TypedDataType::Bytes => match value {
            Value::String(s) => Ok(TypedValue::Bytes(s.as_bytes().to_vec())),
            other => Ok(TypedValue::Bytes(other.to_string().into_bytes())),
        },
        TypedDataType::Date => value
            .as_str()
            .and_then(parse_date)
            .map(TypedValue::Date)
            .ok_or_else(|| mapping_err(col, value, "expected YYYY-MM-DD")),
        TypedDataType::Timestamp => value
            .as_str()
            .and_then(parse_timestamp_micros)
            .map(TypedValue::Timestamp)
            .ok_or_else(|| mapping_err(col, value, "expected YYYY-MM-DD HH:MM:SS[.fff]")),
        TypedDataType::Decimal { .. } => {
            let s = match value {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                other => other.to_string(),
            };
            Ok(TypedValue::Decimal(s))
        }
        TypedDataType::Json => Ok(TypedValue::Json(value.clone())),
        TypedDataType::Unknown => match value {
            Value::String(s) => Ok(TypedValue::Text(s.clone())),
            other => Ok(TypedValue::Text(other.to_string())),
        },
    }
}

fn number_as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn number_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

// ── Date / timestamp parsing (dependency-free, mirrors airhouse_typed) ──────

fn parse_date(s: &str) -> Option<i32> {
    let (y, m, d) = split_ymd(s)?;
    Some(days_from_civil(y, m, d))
}

fn parse_timestamp_micros(s: &str) -> Option<i64> {
    let (date_part, time_part) = match s.split_once(' ') {
        Some((d, t)) => (d, Some(t)),
        None => match s.split_once('T') {
            Some((d, t)) => (d, Some(t)),
            None => (s, None),
        },
    };
    let (y, m, d) = split_ymd(date_part)?;
    let days = days_from_civil(y, m, d) as i64;
    let sod_micros = match time_part {
        None => 0i64,
        Some(t) => parse_time_micros(t)?,
    };
    Some(days * 86_400 * 1_000_000 + sod_micros)
}

fn parse_time_micros(s: &str) -> Option<i64> {
    let s = s
        .split('+')
        .next()
        .unwrap_or(s)
        .trim_end_matches('Z')
        .trim();

    let mut parts = s.splitn(3, ':');
    let h: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let sec_raw = parts.next()?;

    let (sec_i, frac_us) = match sec_raw.split_once('.') {
        Some((whole, frac)) => {
            let sec_i: i64 = whole.parse().ok()?;
            let frac_truncated: String = frac.chars().take(6).collect();
            let frac_padded = format!("{frac_truncated:0<6}");
            let frac_us: i64 = frac_padded.parse().ok()?;
            (sec_i, frac_us)
        }
        None => (sec_raw.parse().ok()?, 0i64),
    };

    Some(h * 3_600_000_000 + m * 60_000_000 + sec_i * 1_000_000 + frac_us)
}

fn split_ymd(s: &str) -> Option<(i64, u32, u32)> {
    let mut parts = s.splitn(3, '-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    Some((y, m, d))
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i32 {
    let (y, m) = if m <= 2 { (y - 1, m + 12) } else { (y, m) };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * (m - 3) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe as i64 - 719_468) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(data_type: TypedDataType) -> ColumnSpec {
        ColumnSpec {
            name: "c".into(),
            data_type,
        }
    }

    #[test]
    fn type_mapping_strips_nullable_and_lowcardinality() {
        assert_eq!(ch_type_to_typed("Nullable(Int32)"), TypedDataType::Int32);
        assert_eq!(
            ch_type_to_typed("LowCardinality(String)"),
            TypedDataType::Text
        );
        assert_eq!(
            ch_type_to_typed("Nullable(LowCardinality(String))"),
            TypedDataType::Text
        );
    }

    #[test]
    fn type_mapping_scalars() {
        assert_eq!(ch_type_to_typed("Bool"), TypedDataType::Bool);
        assert_eq!(ch_type_to_typed("Int32"), TypedDataType::Int32);
        assert_eq!(ch_type_to_typed("Int64"), TypedDataType::Int64);
        assert_eq!(ch_type_to_typed("UInt64"), TypedDataType::Int64);
        assert_eq!(ch_type_to_typed("Float64"), TypedDataType::Float64);
        assert_eq!(ch_type_to_typed("String"), TypedDataType::Text);
        assert_eq!(ch_type_to_typed("UUID"), TypedDataType::Text);
        assert_eq!(ch_type_to_typed("Date"), TypedDataType::Date);
        assert_eq!(ch_type_to_typed("DateTime"), TypedDataType::Timestamp);
        assert_eq!(
            ch_type_to_typed("DateTime64(3, 'UTC')"),
            TypedDataType::Timestamp
        );
    }

    #[test]
    fn type_mapping_decimals() {
        assert_eq!(
            ch_type_to_typed("Decimal(18, 2)"),
            TypedDataType::Decimal {
                precision: 18,
                scale: 2
            }
        );
        assert_eq!(
            ch_type_to_typed("Decimal32(4)"),
            TypedDataType::Decimal {
                precision: 4,
                scale: 0
            }
        );
    }

    #[test]
    fn type_mapping_composites_as_json() {
        assert_eq!(ch_type_to_typed("Array(Int32)"), TypedDataType::Json);
        assert_eq!(
            ch_type_to_typed("Tuple(Int32, String)"),
            TypedDataType::Json
        );
        assert_eq!(ch_type_to_typed("Map(String, Int64)"), TypedDataType::Json);
    }

    #[test]
    fn parse_cell_handles_null_and_bool() {
        assert_eq!(
            parse_ch_cell(&Value::Null, &col(TypedDataType::Bool)).unwrap(),
            TypedValue::Null
        );
        assert_eq!(
            parse_ch_cell(&Value::Bool(true), &col(TypedDataType::Bool)).unwrap(),
            TypedValue::Bool(true)
        );
        assert_eq!(
            parse_ch_cell(&serde_json::json!(1), &col(TypedDataType::Bool)).unwrap(),
            TypedValue::Bool(true)
        );
    }

    #[test]
    fn parse_cell_decodes_ints_from_number_or_string() {
        // Int64 commonly arrives as a string in JSONCompact.
        assert_eq!(
            parse_ch_cell(&Value::String("12345".into()), &col(TypedDataType::Int64)).unwrap(),
            TypedValue::Int64(12345)
        );
        // Int32 arrives as a JSON number.
        assert_eq!(
            parse_ch_cell(&serde_json::json!(42), &col(TypedDataType::Int32)).unwrap(),
            TypedValue::Int32(42)
        );
    }

    #[test]
    fn parse_cell_routes_uint64_overflow_to_decimal_string() {
        // UInt64 max (18446744073709551615) overflows i64.
        let v = Value::String("18446744073709551615".into());
        match parse_ch_cell(&v, &col(TypedDataType::Int64)).unwrap() {
            TypedValue::Decimal(s) => assert_eq!(s, "18446744073709551615"),
            other => panic!("expected Decimal fallback, got {other:?}"),
        }
    }

    #[test]
    fn parse_cell_date_and_timestamp() {
        assert_eq!(
            parse_ch_cell(
                &Value::String("1970-01-01".into()),
                &col(TypedDataType::Date)
            )
            .unwrap(),
            TypedValue::Date(0)
        );
        assert_eq!(
            parse_ch_cell(
                &Value::String("1970-01-01 00:00:00".into()),
                &col(TypedDataType::Timestamp)
            )
            .unwrap(),
            TypedValue::Timestamp(0)
        );
        assert_eq!(
            parse_ch_cell(
                &Value::String("1970-01-01 00:00:01.5".into()),
                &col(TypedDataType::Timestamp)
            )
            .unwrap(),
            TypedValue::Timestamp(1_500_000)
        );
    }

    #[test]
    fn parse_cell_decimal_preserves_string() {
        let v = Value::String("123.4500".into());
        assert_eq!(
            parse_ch_cell(
                &v,
                &col(TypedDataType::Decimal {
                    precision: 10,
                    scale: 4
                })
            )
            .unwrap(),
            TypedValue::Decimal("123.4500".into())
        );
    }

    #[test]
    fn parse_cell_json_passes_through_arrays() {
        let v = serde_json::json!([1, 2, 3]);
        match parse_ch_cell(&v, &col(TypedDataType::Json)).unwrap() {
            TypedValue::Json(j) => assert_eq!(j, v),
            other => panic!("expected Json, got {other:?}"),
        }
    }
}
