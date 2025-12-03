use arrow::array::*;
use arrow::datatypes::{
    DataType, Float32Type, Float64Type, Int8Type, Int16Type, Int32Type, Int64Type, UInt8Type,
    UInt16Type, UInt32Type, UInt64Type,
};
use std::collections::HashSet;

use super::traits::ColumnAnalyzer;
use super::types::{ColumnInfo, ColumnKind};

pub struct DefaultColumnAnalyzer;

impl ColumnAnalyzer for DefaultColumnAnalyzer {
    fn analyze(&self, name: &str, data_type: &DataType, array: &dyn Array) -> ColumnInfo {
        let null_count = array.null_count();
        let row_count = array.len();
        let (kind, unique_count) = self.classify_column(data_type, array, row_count);

        ColumnInfo {
            name: name.to_string(),
            arrow_type: data_type.clone(),
            kind,
            null_count,
            row_count,
            unique_count,
        }
    }
}

impl DefaultColumnAnalyzer {
    fn classify_column(
        &self,
        data_type: &DataType,
        array: &dyn Array,
        row_count: usize,
    ) -> (ColumnKind, usize) {
        match data_type {
            DataType::Int8 => Self::analyze_signed_int::<Int8Type>(array),
            DataType::Int16 => Self::analyze_signed_int::<Int16Type>(array),
            DataType::Int32 => Self::analyze_signed_int::<Int32Type>(array),
            DataType::Int64 => Self::analyze_signed_int::<Int64Type>(array),
            DataType::UInt8 => Self::analyze_unsigned_int::<UInt8Type>(array),
            DataType::UInt16 => Self::analyze_unsigned_int::<UInt16Type>(array),
            DataType::UInt32 => Self::analyze_unsigned_int::<UInt32Type>(array),
            DataType::UInt64 => Self::analyze_unsigned_int::<UInt64Type>(array),
            DataType::Float32 => Self::analyze_float::<Float32Type>(array),
            DataType::Float64 => Self::analyze_float::<Float64Type>(array),
            DataType::Date32 | DataType::Date64 | DataType::Timestamp(_, _) => {
                (ColumnKind::Temporal, row_count)
            }
            DataType::Utf8 => Self::analyze_string::<i32>(array, row_count),
            DataType::LargeUtf8 => Self::analyze_string::<i64>(array, row_count),
            DataType::Boolean => (ColumnKind::Boolean, 2),
            _ => (ColumnKind::Unknown, 0),
        }
    }

    fn analyze_signed_int<T>(array: &dyn Array) -> (ColumnKind, usize)
    where
        T: ArrowPrimitiveType,
        T::Native: Into<i64> + Copy + std::hash::Hash + Eq,
    {
        let arr = array.as_any().downcast_ref::<PrimitiveArray<T>>().unwrap();
        let mut min_val: Option<i64> = None;
        let mut max_val: Option<i64> = None;
        let mut has_negatives = false;
        let mut unique: HashSet<i64> = HashSet::new();

        for i in 0..arr.len() {
            if arr.is_valid(i) {
                let val: i64 = arr.value(i).into();
                unique.insert(val);
                if val < 0 {
                    has_negatives = true;
                }
                min_val = Some(min_val.map_or(val, |m| m.min(val)));
                max_val = Some(max_val.map_or(val, |m| m.max(val)));
            }
        }

        (
            ColumnKind::Numeric {
                is_integer: true,
                has_negatives,
                min: min_val.map(|v| v as f64),
                max: max_val.map(|v| v as f64),
            },
            unique.len(),
        )
    }

    fn analyze_unsigned_int<T>(array: &dyn Array) -> (ColumnKind, usize)
    where
        T: ArrowPrimitiveType,
        T::Native: Into<u64> + Copy + std::hash::Hash + Eq,
    {
        let arr = array.as_any().downcast_ref::<PrimitiveArray<T>>().unwrap();
        let mut min_val: Option<u64> = None;
        let mut max_val: Option<u64> = None;
        let mut unique: HashSet<u64> = HashSet::new();

        for i in 0..arr.len() {
            if arr.is_valid(i) {
                let val: u64 = arr.value(i).into();
                unique.insert(val);
                min_val = Some(min_val.map_or(val, |m| m.min(val)));
                max_val = Some(max_val.map_or(val, |m| m.max(val)));
            }
        }

        (
            ColumnKind::Numeric {
                is_integer: true,
                has_negatives: false,
                min: min_val.map(|v| v as f64),
                max: max_val.map(|v| v as f64),
            },
            unique.len(),
        )
    }

    fn analyze_float<T>(array: &dyn Array) -> (ColumnKind, usize)
    where
        T: ArrowPrimitiveType,
        T::Native: Into<f64> + Copy,
    {
        let arr = array.as_any().downcast_ref::<PrimitiveArray<T>>().unwrap();
        let mut min_val: Option<f64> = None;
        let mut max_val: Option<f64> = None;
        let mut has_negatives = false;
        let mut count: usize = 0;

        for i in 0..arr.len() {
            if arr.is_valid(i) {
                let val: f64 = arr.value(i).into();
                if val.is_finite() {
                    count += 1;
                    if val < 0.0 {
                        has_negatives = true;
                    }
                    min_val = Some(min_val.map_or(val, |m| m.min(val)));
                    max_val = Some(max_val.map_or(val, |m| m.max(val)));
                }
            }
        }

        (
            ColumnKind::Numeric {
                is_integer: false,
                has_negatives,
                min: min_val,
                max: max_val,
            },
            count,
        )
    }

    fn analyze_string<O: OffsetSizeTrait>(
        array: &dyn Array,
        row_count: usize,
    ) -> (ColumnKind, usize) {
        let arr = array
            .as_any()
            .downcast_ref::<GenericStringArray<O>>()
            .unwrap();
        let mut unique: HashSet<String> = HashSet::new();

        for i in 0..arr.len() {
            if arr.is_valid(i) {
                unique.insert(arr.value(i).to_string());
            }
        }

        let cardinality = unique.len();
        let is_categorical = cardinality <= 50 && cardinality <= (row_count / 2).max(2);

        if is_categorical {
            (ColumnKind::Categorical { cardinality }, cardinality)
        } else {
            (ColumnKind::Text, cardinality)
        }
    }
}
