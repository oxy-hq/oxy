//! Snowflake Arrow → CellValue and JSON → CellValue converters.

#![cfg(feature = "snowflake")]

use arrow::array::{
    Array, BooleanArray, Date32Array, Decimal128Array, Float32Array, Float64Array, Int8Array,
    Int16Array, Int32Array, Int64Array, LargeStringArray, StringArray, UInt8Array, UInt16Array,
    UInt32Array, UInt64Array,
};
use arrow::datatypes::DataType;

use agentic_core::result::CellValue;

/// Convert a single cell from an Arrow column array into a [`CellValue`].
pub(super) fn arrow_to_cell(array: &dyn Array, row: usize) -> CellValue {
    if array.is_null(row) {
        return CellValue::Null;
    }

    match array.data_type() {
        DataType::Int8 => array
            .as_any()
            .downcast_ref::<Int8Array>()
            .map(|a| CellValue::Number(a.value(row) as f64))
            .unwrap_or(CellValue::Null),
        DataType::Int16 => array
            .as_any()
            .downcast_ref::<Int16Array>()
            .map(|a| CellValue::Number(a.value(row) as f64))
            .unwrap_or(CellValue::Null),
        DataType::Int32 => array
            .as_any()
            .downcast_ref::<Int32Array>()
            .map(|a| CellValue::Number(a.value(row) as f64))
            .unwrap_or(CellValue::Null),
        DataType::Int64 => array
            .as_any()
            .downcast_ref::<Int64Array>()
            .map(|a| CellValue::Number(a.value(row) as f64))
            .unwrap_or(CellValue::Null),
        DataType::UInt8 => array
            .as_any()
            .downcast_ref::<UInt8Array>()
            .map(|a| CellValue::Number(a.value(row) as f64))
            .unwrap_or(CellValue::Null),
        DataType::UInt16 => array
            .as_any()
            .downcast_ref::<UInt16Array>()
            .map(|a| CellValue::Number(a.value(row) as f64))
            .unwrap_or(CellValue::Null),
        DataType::UInt32 => array
            .as_any()
            .downcast_ref::<UInt32Array>()
            .map(|a| CellValue::Number(a.value(row) as f64))
            .unwrap_or(CellValue::Null),
        DataType::UInt64 => array
            .as_any()
            .downcast_ref::<UInt64Array>()
            .map(|a| CellValue::Number(a.value(row) as f64))
            .unwrap_or(CellValue::Null),
        DataType::Float32 => array
            .as_any()
            .downcast_ref::<Float32Array>()
            .map(|a| CellValue::Number(a.value(row) as f64))
            .unwrap_or(CellValue::Null),
        DataType::Float64 => array
            .as_any()
            .downcast_ref::<Float64Array>()
            .map(|a| CellValue::Number(a.value(row)))
            .unwrap_or(CellValue::Null),
        DataType::Boolean => array
            .as_any()
            .downcast_ref::<BooleanArray>()
            .map(|a| CellValue::Number(if a.value(row) { 1.0 } else { 0.0 }))
            .unwrap_or(CellValue::Null),
        DataType::Utf8 => array
            .as_any()
            .downcast_ref::<StringArray>()
            .map(|a| CellValue::Text(a.value(row).to_string()))
            .unwrap_or(CellValue::Null),
        DataType::LargeUtf8 => array
            .as_any()
            .downcast_ref::<LargeStringArray>()
            .map(|a| CellValue::Text(a.value(row).to_string()))
            .unwrap_or(CellValue::Null),
        DataType::Date32 => array
            .as_any()
            .downcast_ref::<Date32Array>()
            .map(|a| {
                CellValue::Text(a.value_as_date(row).map_or_else(
                    || a.value(row).to_string(),
                    |d| d.format("%Y-%m-%d").to_string(),
                ))
            })
            .unwrap_or(CellValue::Null),
        DataType::Decimal128(_, scale) => {
            let scale = *scale;
            array
                .as_any()
                .downcast_ref::<Decimal128Array>()
                .map(|a| {
                    let raw = a.value(row);
                    if scale == 0 {
                        CellValue::Number(raw as f64)
                    } else {
                        CellValue::Number(raw as f64 / 10f64.powi(scale as i32))
                    }
                })
                .unwrap_or(CellValue::Null)
        }
        _ => CellValue::Text(format!("{:?}", array.data_type())),
    }
}

// ── Connector ─────────────────────────────────────────────────────────────────

/// Snowflake connector.
///
/// Stores connection parameters and creates a fresh [`SnowflakeApi`] per

/// Convert a `serde_json::Value` to a [`CellValue`].
pub(super) fn json_value_to_cell(v: &serde_json::Value) -> CellValue {
    match v {
        serde_json::Value::Null => CellValue::Null,
        serde_json::Value::Bool(b) => CellValue::Number(if *b { 1.0 } else { 0.0 }),
        serde_json::Value::Number(n) => CellValue::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => {
            if let Ok(n) = s.parse::<f64>() {
                CellValue::Number(n)
            } else {
                CellValue::Text(s.clone())
            }
        }
        other => CellValue::Text(other.to_string()),
    }
}
