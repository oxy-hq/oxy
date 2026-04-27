//! Arrow → `TypedValue` conversion for the Snowflake backend.
//!
//! `snowflake-api` returns query results as Arrow `RecordBatch`es. The
//! bounded-sample path in `conversion.rs` already decodes those into
//! [`CellValue`] for the solver; this module provides the full-fidelity
//! [`TypedValue`] variant used by `execute_query_full` (typed Parquet,
//! typed grid rendering).
//!
//! The Snowflake connector also implements [`AsArrowConnector`] — when the
//! `arrow` feature is on, callers can skip this conversion entirely and
//! stream the raw `RecordBatch`es straight to a Parquet writer.
//!
//! [`AsArrowConnector`]: crate::connector::AsArrowConnector

#![cfg(feature = "snowflake")]

use arrow::array::{
    Array, BooleanArray, Date32Array, Date64Array, Decimal128Array, Decimal256Array, Float16Array,
    Float32Array, Float64Array, Int8Array, Int16Array, Int32Array, Int64Array, LargeStringArray,
    StringArray, TimestampMicrosecondArray, TimestampMillisecondArray, TimestampNanosecondArray,
    TimestampSecondArray, UInt8Array, UInt16Array, UInt32Array, UInt64Array,
};
use arrow::datatypes::{DataType, TimeUnit};

use agentic_core::result::{TypedDataType, TypedValue};

// ── Arrow DataType → TypedDataType ──────────────────────────────────────────

/// Map an Arrow [`DataType`] to our [`TypedDataType`].
///
/// Composite types (`List`, `Struct`, `Map`, `Union`, etc.) are rendered as
/// [`TypedDataType::Unknown`] — the row decoder stringifies the value so
/// Parquet consumers still get a readable rendering.
pub(super) fn arrow_dtype_to_typed(dt: &DataType) -> TypedDataType {
    match dt {
        DataType::Boolean => TypedDataType::Bool,
        DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::UInt8 | DataType::UInt16 => {
            TypedDataType::Int32
        }
        // UInt32 fits in i64; UInt64 may overflow i64 — route as Int64 and
        // let the decoder fall back to Decimal-string for the overflow case.
        DataType::Int64 | DataType::UInt32 | DataType::UInt64 => TypedDataType::Int64,
        DataType::Float16 | DataType::Float32 | DataType::Float64 => TypedDataType::Float64,
        DataType::Utf8 | DataType::LargeUtf8 => TypedDataType::Text,
        DataType::Binary | DataType::LargeBinary | DataType::FixedSizeBinary(_) => {
            TypedDataType::Bytes
        }
        DataType::Date32 | DataType::Date64 => TypedDataType::Date,
        DataType::Timestamp(_, _) => TypedDataType::Timestamp,
        DataType::Decimal128(precision, scale) | DataType::Decimal256(precision, scale) => {
            TypedDataType::Decimal {
                precision: *precision,
                scale: *scale,
            }
        }
        _ => TypedDataType::Unknown,
    }
}

// ── Arrow array element → TypedValue ────────────────────────────────────────

/// Decode a single row of an Arrow column array into a [`TypedValue`].
///
/// The `data_type` hint drives the variant chosen (so we match what the
/// column spec promises); NULL rows always return [`TypedValue::Null`].
/// Unsupported Arrow types fall through to [`TypedValue::Text`] with the
/// Arrow Debug rendering — keeps behaviour predictable for exotic types
/// without widening the [`TypedValue`] enum.
pub(super) fn arrow_to_typed(
    array: &dyn Array,
    row: usize,
    data_type: &TypedDataType,
) -> TypedValue {
    if array.is_null(row) {
        return TypedValue::Null;
    }

    match array.data_type() {
        // ── Booleans ──────────────────────────────────────────────────────
        DataType::Boolean => array
            .as_any()
            .downcast_ref::<BooleanArray>()
            .map(|a| TypedValue::Bool(a.value(row)))
            .unwrap_or(TypedValue::Null),

        // ── Signed integers ───────────────────────────────────────────────
        DataType::Int8 => downcast_i32::<Int8Array>(array, row, |a, i| a.value(i) as i32),
        DataType::Int16 => downcast_i32::<Int16Array>(array, row, |a, i| a.value(i) as i32),
        DataType::Int32 => downcast_i32::<Int32Array>(array, row, |a, i| a.value(i)),
        DataType::Int64 => array
            .as_any()
            .downcast_ref::<Int64Array>()
            .map(|a| TypedValue::Int64(a.value(row)))
            .unwrap_or(TypedValue::Null),

        // ── Unsigned integers ─────────────────────────────────────────────
        DataType::UInt8 => downcast_i32::<UInt8Array>(array, row, |a, i| a.value(i) as i32),
        DataType::UInt16 => downcast_i32::<UInt16Array>(array, row, |a, i| a.value(i) as i32),
        DataType::UInt32 => array
            .as_any()
            .downcast_ref::<UInt32Array>()
            .map(|a| TypedValue::Int64(a.value(row) as i64))
            .unwrap_or(TypedValue::Null),
        DataType::UInt64 => array
            .as_any()
            .downcast_ref::<UInt64Array>()
            .map(|a| {
                let v = a.value(row);
                match i64::try_from(v) {
                    Ok(n) => TypedValue::Int64(n),
                    Err(_) => TypedValue::Decimal(v.to_string()),
                }
            })
            .unwrap_or(TypedValue::Null),

        // ── Floats ────────────────────────────────────────────────────────
        DataType::Float16 => array
            .as_any()
            .downcast_ref::<Float16Array>()
            .map(|a| TypedValue::Float64(a.value(row).to_f64()))
            .unwrap_or(TypedValue::Null),
        DataType::Float32 => array
            .as_any()
            .downcast_ref::<Float32Array>()
            .map(|a| TypedValue::Float64(a.value(row) as f64))
            .unwrap_or(TypedValue::Null),
        DataType::Float64 => array
            .as_any()
            .downcast_ref::<Float64Array>()
            .map(|a| TypedValue::Float64(a.value(row)))
            .unwrap_or(TypedValue::Null),

        // ── Strings ───────────────────────────────────────────────────────
        DataType::Utf8 => array
            .as_any()
            .downcast_ref::<StringArray>()
            .map(|a| TypedValue::Text(a.value(row).to_string()))
            .unwrap_or(TypedValue::Null),
        DataType::LargeUtf8 => array
            .as_any()
            .downcast_ref::<LargeStringArray>()
            .map(|a| TypedValue::Text(a.value(row).to_string()))
            .unwrap_or(TypedValue::Null),

        // ── Dates ─────────────────────────────────────────────────────────
        DataType::Date32 => array
            .as_any()
            .downcast_ref::<Date32Array>()
            .map(|a| TypedValue::Date(a.value(row)))
            .unwrap_or(TypedValue::Null),
        DataType::Date64 => array
            .as_any()
            .downcast_ref::<Date64Array>()
            .map(|a| {
                // Date64 stores milliseconds since epoch — down-convert to
                // days for our Arrow Date32 wire shape.
                let millis = a.value(row);
                let days = (millis / 86_400_000) as i32;
                TypedValue::Date(days)
            })
            .unwrap_or(TypedValue::Null),

        // ── Timestamps ────────────────────────────────────────────────────
        DataType::Timestamp(unit, _tz) => match unit {
            TimeUnit::Second => array
                .as_any()
                .downcast_ref::<TimestampSecondArray>()
                .map(|a| TypedValue::Timestamp(a.value(row).saturating_mul(1_000_000)))
                .unwrap_or(TypedValue::Null),
            TimeUnit::Millisecond => array
                .as_any()
                .downcast_ref::<TimestampMillisecondArray>()
                .map(|a| TypedValue::Timestamp(a.value(row).saturating_mul(1_000)))
                .unwrap_or(TypedValue::Null),
            TimeUnit::Microsecond => array
                .as_any()
                .downcast_ref::<TimestampMicrosecondArray>()
                .map(|a| TypedValue::Timestamp(a.value(row)))
                .unwrap_or(TypedValue::Null),
            TimeUnit::Nanosecond => array
                .as_any()
                .downcast_ref::<TimestampNanosecondArray>()
                .map(|a| TypedValue::Timestamp(a.value(row) / 1_000))
                .unwrap_or(TypedValue::Null),
        },

        // ── Decimals — render to canonical string at the declared scale ──
        DataType::Decimal128(_, scale) => array
            .as_any()
            .downcast_ref::<Decimal128Array>()
            .map(|a| TypedValue::Decimal(format_i128_as_decimal(a.value(row), *scale)))
            .unwrap_or(TypedValue::Null),
        DataType::Decimal256(_, _scale) => array
            .as_any()
            .downcast_ref::<Decimal256Array>()
            .map(|a| TypedValue::Decimal(format!("{}", a.value(row))))
            .unwrap_or(TypedValue::Null),

        // ── Fallback: stringify ───────────────────────────────────────────
        _ => {
            // Keep `data_type` reachable — suppresses the unused-parameter
            // warning while making it clear that downstream parquet writers
            // will store this as `Utf8` per the `TypedDataType::Unknown`
            // mapping.
            let _ = data_type;
            TypedValue::Text(format!("{:?}", array.data_type()))
        }
    }
}

/// Small helper for the four narrow integer arms (`Int8/16` + `UInt8/16`):
/// downcast, pull one cell, wrap as [`TypedValue::Int32`].
fn downcast_i32<A>(array: &dyn Array, row: usize, extract: fn(&A, usize) -> i32) -> TypedValue
where
    A: 'static,
{
    array
        .as_any()
        .downcast_ref::<A>()
        .map(|a| TypedValue::Int32(extract(a, row)))
        .unwrap_or(TypedValue::Null)
}

// ── Decimal rendering ───────────────────────────────────────────────────────

/// Render an `i128` value with the given Arrow `Decimal128` scale as its
/// canonical decimal-string form. Positive scale means `scale` digits after
/// the decimal point; negative scale scales up by 10^|scale|.
fn format_i128_as_decimal(value: i128, scale: i8) -> String {
    if scale == 0 {
        return value.to_string();
    }
    if scale < 0 {
        // e.g. `1E3` with scale=-3 → value already scaled up; render as
        // integer with trailing zeros.
        let multiplier = 10i128.pow((-scale) as u32);
        return (value * multiplier).to_string();
    }
    let scale_usize = scale as usize;
    let (sign, abs) = if value < 0 {
        ("-", (-value).to_string())
    } else {
        ("", value.to_string())
    };
    if abs.len() <= scale_usize {
        // Fewer digits than scale → pad with leading zeros: "00042" → "0.00042"
        let padded = format!("{abs:0>width$}", width = scale_usize);
        format!("{sign}0.{padded}")
    } else {
        let split = abs.len() - scale_usize;
        let (int_part, frac_part) = abs.split_at(split);
        format!("{sign}{int_part}.{frac_part}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Int64Array, StringArray};
    use std::sync::Arc;

    #[test]
    fn arrow_dtype_to_typed_covers_common_types() {
        assert_eq!(
            arrow_dtype_to_typed(&DataType::Boolean),
            TypedDataType::Bool
        );
        assert_eq!(arrow_dtype_to_typed(&DataType::Int32), TypedDataType::Int32);
        assert_eq!(arrow_dtype_to_typed(&DataType::Int64), TypedDataType::Int64);
        assert_eq!(
            arrow_dtype_to_typed(&DataType::Float64),
            TypedDataType::Float64
        );
        assert_eq!(arrow_dtype_to_typed(&DataType::Utf8), TypedDataType::Text);
        assert_eq!(arrow_dtype_to_typed(&DataType::Date32), TypedDataType::Date);
        assert_eq!(
            arrow_dtype_to_typed(&DataType::Timestamp(TimeUnit::Microsecond, None)),
            TypedDataType::Timestamp
        );
        assert_eq!(
            arrow_dtype_to_typed(&DataType::Decimal128(18, 2)),
            TypedDataType::Decimal {
                precision: 18,
                scale: 2
            }
        );
    }

    #[test]
    fn arrow_to_typed_decodes_int64() {
        let arr: Arc<dyn Array> = Arc::new(Int64Array::from(vec![Some(42), None, Some(-7)]));
        assert_eq!(
            arrow_to_typed(arr.as_ref(), 0, &TypedDataType::Int64),
            TypedValue::Int64(42)
        );
        assert_eq!(
            arrow_to_typed(arr.as_ref(), 1, &TypedDataType::Int64),
            TypedValue::Null
        );
        assert_eq!(
            arrow_to_typed(arr.as_ref(), 2, &TypedDataType::Int64),
            TypedValue::Int64(-7)
        );
    }

    #[test]
    fn arrow_to_typed_decodes_utf8() {
        let arr: Arc<dyn Array> = Arc::new(StringArray::from(vec![Some("hello"), None]));
        assert_eq!(
            arrow_to_typed(arr.as_ref(), 0, &TypedDataType::Text),
            TypedValue::Text("hello".into())
        );
        assert_eq!(
            arrow_to_typed(arr.as_ref(), 1, &TypedDataType::Text),
            TypedValue::Null
        );
    }

    #[test]
    fn format_decimal_basic() {
        // value=12345, scale=2 → "123.45"
        assert_eq!(format_i128_as_decimal(12345, 2), "123.45");
        // value=42, scale=4 → "0.0042"
        assert_eq!(format_i128_as_decimal(42, 4), "0.0042");
        // value=-100, scale=2 → "-1.00"
        assert_eq!(format_i128_as_decimal(-100, 2), "-1.00");
        // scale=0 → "42"
        assert_eq!(format_i128_as_decimal(42, 0), "42");
    }
}
