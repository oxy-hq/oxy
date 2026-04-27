//! Row-oriented typed conversion helpers for the BigQuery backend.
//!
//! [`BigQueryConnector::execute_query_full`] uses these to:
//!
//! 1. Map the BigQuery `FieldType` on each column into a [`TypedDataType`].
//! 2. Decode each row's cell out of `ResultSet` using the typed accessor
//!    appropriate for that column (`get_i64_by_name`, `get_bool_by_name`,
//!    `get_f64_by_name`, `get_string_by_name`, `get_json_value_by_name`).
//!
//! REPEATED (array) fields and composite RECORD / STRUCT fields are always
//! rendered as [`TypedDataType::Json`] — the wire form is a JSON array /
//! object, which `get_json_value_by_name` threads through unchanged.

#![cfg(feature = "bigquery")]

use base64::Engine;
use gcp_bigquery_client::model::field_type::FieldType;
use gcp_bigquery_client::model::query_response::ResultSet;
use gcp_bigquery_client::model::table_field_schema::TableFieldSchema;

use agentic_core::result::{ColumnSpec, TypedDataType, TypedRowError, TypedValue};

// ── Type mapping: BigQuery FieldType → TypedDataType ────────────────────────

/// Map a BigQuery column's [`FieldType`] (and its `mode` — e.g. "REPEATED")
/// into our [`TypedDataType`].
///
/// `REPEATED` columns map to [`TypedDataType::Json`] regardless of the
/// element type; consumers that need the array shape walk the JSON value
/// directly.
pub(crate) fn bq_field_to_typed(field: &TableFieldSchema) -> TypedDataType {
    let is_repeated = field
        .mode
        .as_deref()
        .map(|m| m.eq_ignore_ascii_case("REPEATED"))
        .unwrap_or(false);
    if is_repeated {
        return TypedDataType::Json;
    }

    match field.r#type {
        FieldType::String | FieldType::Geography | FieldType::Time | FieldType::Interval => {
            TypedDataType::Text
        }
        FieldType::Bytes => TypedDataType::Bytes,
        FieldType::Integer | FieldType::Int64 => TypedDataType::Int64,
        FieldType::Float | FieldType::Float64 => TypedDataType::Float64,
        FieldType::Numeric | FieldType::Bignumeric => TypedDataType::Decimal {
            precision: 38,
            scale: 0,
        },
        FieldType::Boolean | FieldType::Bool => TypedDataType::Bool,
        // BigQuery TIMESTAMP is a UTC instant; DATETIME is civil date-time.
        // Both normalise into `TypedDataType::Timestamp` (microseconds UTC).
        FieldType::Timestamp | FieldType::Datetime => TypedDataType::Timestamp,
        FieldType::Date => TypedDataType::Date,
        FieldType::Record | FieldType::Struct | FieldType::Json => TypedDataType::Json,
    }
}

// ── Row decoding ────────────────────────────────────────────────────────────

/// Decode one row of a BigQuery `ResultSet` into a vector of [`TypedValue`]s.
pub(crate) fn decode_bq_row(
    rs: &ResultSet,
    columns: &[ColumnSpec],
) -> Result<Vec<TypedValue>, TypedRowError> {
    let mut out = Vec::with_capacity(columns.len());
    for col in columns {
        out.push(decode_cell(rs, col)?);
    }
    Ok(out)
}

fn decode_cell(rs: &ResultSet, col: &ColumnSpec) -> Result<TypedValue, TypedRowError> {
    fn mapping_err(col: &ColumnSpec, err: impl std::fmt::Display) -> TypedRowError {
        TypedRowError::TypeMappingError {
            column: col.name.clone(),
            native_type: format!("{:?}", col.data_type),
            message: err.to_string(),
        }
    }

    match &col.data_type {
        TypedDataType::Bool => match rs.get_bool_by_name(&col.name) {
            Ok(Some(v)) => Ok(TypedValue::Bool(v)),
            Ok(None) => Ok(TypedValue::Null),
            Err(e) => Err(mapping_err(col, e)),
        },
        TypedDataType::Int32 => match rs.get_i64_by_name(&col.name) {
            Ok(Some(v)) => i32::try_from(v)
                .map(TypedValue::Int32)
                .map_err(|_| mapping_err(col, "value overflows i32")),
            Ok(None) => Ok(TypedValue::Null),
            Err(e) => Err(mapping_err(col, e)),
        },
        TypedDataType::Int64 => match rs.get_i64_by_name(&col.name) {
            Ok(Some(v)) => Ok(TypedValue::Int64(v)),
            Ok(None) => Ok(TypedValue::Null),
            Err(e) => Err(mapping_err(col, e)),
        },
        TypedDataType::Float64 => match rs.get_f64_by_name(&col.name) {
            Ok(Some(v)) => Ok(TypedValue::Float64(v)),
            Ok(None) => Ok(TypedValue::Null),
            Err(e) => Err(mapping_err(col, e)),
        },
        TypedDataType::Text => match rs.get_string_by_name(&col.name) {
            Ok(Some(v)) => Ok(TypedValue::Text(v)),
            Ok(None) => Ok(TypedValue::Null),
            Err(e) => Err(mapping_err(col, e)),
        },
        // BigQuery BYTES come as base64-encoded strings.
        TypedDataType::Bytes => match rs.get_string_by_name(&col.name) {
            Ok(Some(s)) => base64::engine::general_purpose::STANDARD
                .decode(s.as_bytes())
                .map(TypedValue::Bytes)
                .map_err(|e| mapping_err(col, format!("invalid base64: {e}"))),
            Ok(None) => Ok(TypedValue::Null),
            Err(e) => Err(mapping_err(col, e)),
        },
        TypedDataType::Date => match rs.get_string_by_name(&col.name) {
            Ok(Some(s)) => parse_date(&s)
                .map(TypedValue::Date)
                .ok_or_else(|| mapping_err(col, "expected YYYY-MM-DD")),
            Ok(None) => Ok(TypedValue::Null),
            Err(e) => Err(mapping_err(col, e)),
        },
        TypedDataType::Timestamp => {
            // BigQuery TIMESTAMP arrives as a floating-point string of seconds
            // since epoch (with fractional subseconds). DATETIME arrives as a
            // civil "YYYY-MM-DDTHH:MM:SS[.ffffff]" string. Try the numeric
            // path first; fall back to string parsing for DATETIME.
            if let Ok(Some(secs)) = rs.get_f64_by_name(&col.name) {
                let micros = (secs * 1_000_000.0).round() as i64;
                return Ok(TypedValue::Timestamp(micros));
            }
            match rs.get_string_by_name(&col.name) {
                Ok(Some(s)) => parse_timestamp_micros(&s)
                    .map(TypedValue::Timestamp)
                    .ok_or_else(|| mapping_err(col, "expected ISO datetime")),
                Ok(None) => Ok(TypedValue::Null),
                Err(e) => Err(mapping_err(col, e)),
            }
        }
        // BigQuery NUMERIC / BIGNUMERIC: preserve exactly as the canonical
        // decimal string — lossless round-trip through to Parquet.
        TypedDataType::Decimal { .. } => match rs.get_string_by_name(&col.name) {
            Ok(Some(s)) => Ok(TypedValue::Decimal(s)),
            Ok(None) => Ok(TypedValue::Null),
            Err(e) => Err(mapping_err(col, e)),
        },
        TypedDataType::Json => match rs.get_json_value_by_name(&col.name) {
            Ok(Some(v)) => Ok(TypedValue::Json(v)),
            Ok(None) => Ok(TypedValue::Null),
            Err(e) => Err(mapping_err(col, e)),
        },
        TypedDataType::Unknown => match rs.get_string_by_name(&col.name) {
            Ok(Some(v)) => Ok(TypedValue::Text(v)),
            Ok(None) => Ok(TypedValue::Null),
            Err(e) => Err(mapping_err(col, e)),
        },
    }
}

// ── Date / timestamp parsing (dependency-free, mirrors airhouse_typed) ──────

fn parse_date(s: &str) -> Option<i32> {
    let (y, m, d) = split_ymd(s)?;
    Some(days_from_civil(y, m, d))
}

fn parse_timestamp_micros(s: &str) -> Option<i64> {
    // BigQuery DATETIME: "YYYY-MM-DDTHH:MM:SS[.ffffff]" or
    // "YYYY-MM-DD HH:MM:SS[.ffffff]".
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
    use gcp_bigquery_client::model::field_type::FieldType;
    use gcp_bigquery_client::model::table_field_schema::TableFieldSchema;

    fn field(ty: FieldType, mode: Option<&str>) -> TableFieldSchema {
        let mut f = TableFieldSchema::new("c", ty);
        f.mode = mode.map(|m| m.to_string());
        f
    }

    #[test]
    fn field_mapping_scalars() {
        assert_eq!(
            bq_field_to_typed(&field(FieldType::Bool, None)),
            TypedDataType::Bool
        );
        assert_eq!(
            bq_field_to_typed(&field(FieldType::Int64, None)),
            TypedDataType::Int64
        );
        assert_eq!(
            bq_field_to_typed(&field(FieldType::Float64, None)),
            TypedDataType::Float64
        );
        assert_eq!(
            bq_field_to_typed(&field(FieldType::String, None)),
            TypedDataType::Text
        );
        assert_eq!(
            bq_field_to_typed(&field(FieldType::Bytes, None)),
            TypedDataType::Bytes
        );
        assert_eq!(
            bq_field_to_typed(&field(FieldType::Date, None)),
            TypedDataType::Date
        );
        assert_eq!(
            bq_field_to_typed(&field(FieldType::Timestamp, None)),
            TypedDataType::Timestamp
        );
        assert_eq!(
            bq_field_to_typed(&field(FieldType::Datetime, None)),
            TypedDataType::Timestamp
        );
        assert_eq!(
            bq_field_to_typed(&field(FieldType::Json, None)),
            TypedDataType::Json
        );
    }

    #[test]
    fn field_mapping_decimal() {
        assert!(matches!(
            bq_field_to_typed(&field(FieldType::Numeric, None)),
            TypedDataType::Decimal { .. }
        ));
        assert!(matches!(
            bq_field_to_typed(&field(FieldType::Bignumeric, None)),
            TypedDataType::Decimal { .. }
        ));
    }

    #[test]
    fn field_mapping_record_and_repeated_go_json() {
        assert_eq!(
            bq_field_to_typed(&field(FieldType::Record, None)),
            TypedDataType::Json
        );
        // REPEATED on ANY scalar → Json (array shape).
        assert_eq!(
            bq_field_to_typed(&field(FieldType::Int64, Some("REPEATED"))),
            TypedDataType::Json
        );
        assert_eq!(
            bq_field_to_typed(&field(FieldType::String, Some("repeated"))),
            TypedDataType::Json
        );
    }

    #[test]
    fn parse_date_basic() {
        assert_eq!(parse_date("1970-01-01"), Some(0));
        assert_eq!(parse_date("2026-04-22"), Some(20_565));
        assert_eq!(parse_date("not-a-date"), None);
    }

    #[test]
    fn parse_timestamp_basic() {
        assert_eq!(parse_timestamp_micros("1970-01-01T00:00:00"), Some(0));
        assert_eq!(
            parse_timestamp_micros("1970-01-01T00:00:01.500000"),
            Some(1_500_000)
        );
        // Space-separated form from DATETIME.
        assert_eq!(parse_timestamp_micros("1970-01-01 00:00:00"), Some(0));
    }
}
