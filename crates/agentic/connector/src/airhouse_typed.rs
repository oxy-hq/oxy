//! Row-oriented typed conversion helpers for the Airhouse backend.
//!
//! Airhouse speaks the PostgreSQL wire protocol but rejects the extended
//! (prepared-statement) protocol that [`crate::postgres`] relies on. Every
//! driver call therefore goes through `simple_query` — which returns every
//! value as a text string. This module maps a DuckDB `DESCRIBE` type string
//! to a [`TypedDataType`] and parses the text cell into a [`TypedValue`]
//! according to that type.
//!
//! The DuckDB type-string parser is intentionally duplicated from the
//! `duckdb` backend (`crates/agentic/connector/src/duckdb/conversion.rs`)
//! rather than imported. Airhouse is compiled without `duckdb`, so sharing
//! would require a cross-feature module. The helper is ~50 lines and has no
//! runtime dependencies; the duplication is preferable to the coupling.

use agentic_core::result::{ColumnSpec, TypedDataType, TypedRowError, TypedValue};

// ── Type mapping: DuckDB DESCRIBE typename → TypedDataType ──────────────────

/// Parse a DuckDB-style type string (as emitted by `DESCRIBE`) into a
/// [`TypedDataType`]. Kept in sync with the DuckDB module's analogue.
pub(crate) fn describe_type_to_typed(type_str: &str) -> TypedDataType {
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

// ── Text → TypedValue parsing ───────────────────────────────────────────────

/// Parse a single cell string (from the simple-query protocol) into a
/// [`TypedValue`] according to the column's declared DuckDB type.
///
/// NULL is represented by `None` on the caller side — this function only sees
/// concrete string renderings.
pub(crate) fn parse_cell(text: &str, col: &ColumnSpec) -> Result<TypedValue, TypedRowError> {
    fn mapping_err(col: &ColumnSpec, text: &str, detail: impl std::fmt::Display) -> TypedRowError {
        TypedRowError::TypeMappingError {
            column: col.name.clone(),
            native_type: format!("{:?}", col.data_type),
            message: format!("could not parse '{text}': {detail}"),
        }
    }

    match &col.data_type {
        TypedDataType::Bool => match text.to_ascii_lowercase().as_str() {
            "t" | "true" | "1" => Ok(TypedValue::Bool(true)),
            "f" | "false" | "0" => Ok(TypedValue::Bool(false)),
            _ => Err(mapping_err(col, text, "unrecognised bool literal")),
        },
        TypedDataType::Int32 => text
            .parse::<i32>()
            .map(TypedValue::Int32)
            .map_err(|e| mapping_err(col, text, e)),
        TypedDataType::Int64 => text
            .parse::<i64>()
            .map(TypedValue::Int64)
            .map_err(|e| mapping_err(col, text, e)),
        TypedDataType::Float64 => text
            .parse::<f64>()
            .map(TypedValue::Float64)
            .map_err(|e| mapping_err(col, text, e)),
        TypedDataType::Decimal { .. } => Ok(TypedValue::Decimal(text.to_string())),
        TypedDataType::Text => Ok(TypedValue::Text(text.to_string())),
        TypedDataType::Bytes => Ok(TypedValue::Bytes(text.as_bytes().to_vec())),
        TypedDataType::Date => parse_date(text)
            .map(TypedValue::Date)
            .ok_or_else(|| mapping_err(col, text, "expected YYYY-MM-DD")),
        TypedDataType::Timestamp => parse_timestamp_micros(text)
            .map(TypedValue::Timestamp)
            .ok_or_else(|| mapping_err(col, text, "expected YYYY-MM-DD[ HH:MM:SS[.fff]]")),
        TypedDataType::Json => serde_json::from_str(text)
            .map(TypedValue::Json)
            .map_err(|e| mapping_err(col, text, e)),
        TypedDataType::Unknown => Ok(TypedValue::Text(text.to_string())),
    }
}

// ── Date / timestamp parsing (deliberately dependency-free) ─────────────────

/// Parse `YYYY-MM-DD` into days since `1970-01-01` (Arrow `Date32`).
fn parse_date(s: &str) -> Option<i32> {
    let (y, m, d) = split_ymd(s)?;
    Some(days_from_civil(y, m, d))
}

/// Parse `YYYY-MM-DD[ HH:MM:SS[.fffffff]]` into microseconds since the Unix
/// epoch (Arrow `Timestamp(Microsecond, UTC)`).
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
    let seconds_of_day_micros = match time_part {
        None => 0i64,
        Some(t) => parse_time_micros(t)?,
    };
    Some(days * 86_400 * 1_000_000 + seconds_of_day_micros)
}

fn parse_time_micros(s: &str) -> Option<i64> {
    // `HH:MM:SS[.fffffff]`; discard any trailing timezone suffix ("+00",
    // "Z", …) since Airhouse casts to `VARCHAR` without zone info in
    // practice — but we tolerate it defensively.
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

/// https://howardhinnant.github.io/date_algorithms.html — days_from_civil.
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
    fn describe_typename_basic() {
        assert_eq!(describe_type_to_typed("INTEGER"), TypedDataType::Int32);
        assert_eq!(describe_type_to_typed("BIGINT"), TypedDataType::Int64);
        assert_eq!(describe_type_to_typed("DOUBLE"), TypedDataType::Float64);
        assert_eq!(describe_type_to_typed("VARCHAR"), TypedDataType::Text);
        assert_eq!(describe_type_to_typed("DATE"), TypedDataType::Date);
        assert_eq!(
            describe_type_to_typed("TIMESTAMP"),
            TypedDataType::Timestamp
        );
        assert_eq!(
            describe_type_to_typed("DECIMAL(9,2)"),
            TypedDataType::Decimal {
                precision: 9,
                scale: 2
            }
        );
    }

    #[test]
    fn parse_cell_integers() {
        assert_eq!(
            parse_cell("42", &col(TypedDataType::Int32)).unwrap(),
            TypedValue::Int32(42)
        );
        assert_eq!(
            parse_cell("-7", &col(TypedDataType::Int64)).unwrap(),
            TypedValue::Int64(-7)
        );
    }

    #[test]
    fn parse_cell_bool() {
        assert_eq!(
            parse_cell("true", &col(TypedDataType::Bool)).unwrap(),
            TypedValue::Bool(true)
        );
        assert_eq!(
            parse_cell("f", &col(TypedDataType::Bool)).unwrap(),
            TypedValue::Bool(false)
        );
        assert!(parse_cell("nope", &col(TypedDataType::Bool)).is_err());
    }

    #[test]
    fn parse_cell_float_and_decimal() {
        assert_eq!(
            parse_cell("3.14", &col(TypedDataType::Float64)).unwrap(),
            TypedValue::Float64(3.14)
        );
        assert_eq!(
            parse_cell(
                "123.45",
                &col(TypedDataType::Decimal {
                    precision: 10,
                    scale: 2
                })
            )
            .unwrap(),
            TypedValue::Decimal("123.45".into())
        );
    }

    #[test]
    fn parse_cell_date_and_timestamp() {
        // 1970-01-01 → 0 days.
        assert_eq!(
            parse_cell("1970-01-01", &col(TypedDataType::Date)).unwrap(),
            TypedValue::Date(0)
        );
        // 2026-04-22 → days since epoch (matches chrono, spot-checked).
        let days_2026 = match parse_cell("2026-04-22", &col(TypedDataType::Date)).unwrap() {
            TypedValue::Date(d) => d,
            _ => unreachable!(),
        };
        assert!(days_2026 > 20_000 && days_2026 < 21_000);

        // 1970-01-01 00:00:00 → 0 micros.
        assert_eq!(
            parse_cell("1970-01-01 00:00:00", &col(TypedDataType::Timestamp)).unwrap(),
            TypedValue::Timestamp(0)
        );
        // Sub-second precision.
        let ts = match parse_cell("1970-01-01 00:00:01.5", &col(TypedDataType::Timestamp)).unwrap()
        {
            TypedValue::Timestamp(t) => t,
            _ => unreachable!(),
        };
        assert_eq!(ts, 1_500_000);
    }

    #[test]
    fn parse_cell_json() {
        let v = parse_cell("{\"a\": 1}", &col(TypedDataType::Json)).unwrap();
        match v {
            TypedValue::Json(j) => {
                assert_eq!(j, serde_json::json!({"a": 1}));
            }
            _ => panic!("expected json"),
        }
    }
}
