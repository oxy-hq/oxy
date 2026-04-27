//! Snowflake schema introspection and join-key detection helpers.

#![cfg(feature = "snowflake")]

use std::collections::HashMap;

use agentic_core::result::CellValue;
use snowflake_api::QueryResult as SnowflakeQueryResult;

use crate::config::SnowflakeAuth;
use crate::connector::{ConnectorError, SchemaColumnInfo, SchemaInfo, SchemaTableInfo};

use super::build_api;
use super::conversion::{arrow_to_cell, json_value_to_cell};

pub(super) fn extract_count(result: SnowflakeQueryResult) -> u64 {
    match result {
        SnowflakeQueryResult::Arrow(batches) => batches
            .first()
            .and_then(|b| {
                if b.num_rows() > 0 && b.num_columns() > 0 {
                    let cell = arrow_to_cell(b.column(0).as_ref(), 0);
                    if let CellValue::Number(n) = cell {
                        Some(n as u64)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .unwrap_or(0),
        SnowflakeQueryResult::Json(rows) => rows
            .value
            .as_array()
            .and_then(|outer| outer.first())
            .and_then(|row| row.as_array())
            .and_then(|vals| vals.first())
            .and_then(|v| match v {
                serde_json::Value::Number(n) => n.as_u64(),
                serde_json::Value::String(s) => s.parse().ok(),
                _ => None,
            })
            .unwrap_or(0),
        SnowflakeQueryResult::Empty => 0,
    }
}

/// Categorize a column type string for per-column stats SQL routing.
///
/// Snowflake's `TRY_TO_DOUBLE` / `TRY_CAST(... AS FLOAT)` only accept VARCHAR
/// input — calling it on a numeric column like `NUMBER(18,0)` raises
/// `SQL compilation error: Function TRY_CAST cannot be used with arguments
/// of types NUMBER(18,0) and FLOAT`. So `AVG` / `STDDEV_POP` need a
/// different expression depending on the column's type.
///
/// The caller passes whatever type string it has on hand — Snowflake native
/// (`NUMBER(18,0)`, `VARCHAR(16777216)`, `TIMESTAMP_NTZ(9)`) or the Arrow
/// `DataType` Display form (`Int64`, `Float64`, `Utf8`, `Decimal128(38, 0)`).
/// Both shapes are handled after paren-stripping + uppercasing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TypeCategory {
    /// Numeric types — AVG/STDDEV can read the column directly.
    Numeric,
    /// String/binary types — must be parsed to double first.
    String,
    /// Everything else (dates, times, booleans, semi-structured, or unknown).
    /// Mean / std_dev are emitted as NULL.
    Other,
}

/// Normalize a type string and bucket it into [`TypeCategory`].
pub(super) fn snowflake_type_category(raw: &str) -> TypeCategory {
    // Strip parenthesized precision/scale and uppercase.
    let normalized = raw
        .split('(')
        .next()
        .unwrap_or(raw)
        .trim()
        .to_ascii_uppercase();

    // Snowflake native numerics + Arrow `DataType` Display forms.
    match normalized.as_str() {
        // Snowflake native
        "NUMBER" | "DECIMAL" | "NUMERIC" | "INT" | "INTEGER" | "BIGINT" | "SMALLINT"
        | "TINYINT" | "BYTEINT" | "FLOAT" | "FLOAT4" | "FLOAT8" | "DOUBLE" | "DOUBLE PRECISION"
        | "REAL" => return TypeCategory::Numeric,
        // Arrow Display
        "INT8" | "INT16" | "INT32" | "INT64" | "UINT8" | "UINT16" | "UINT32" | "UINT64"
        | "FLOAT16" | "FLOAT32" | "FLOAT64" => return TypeCategory::Numeric,
        _ => {}
    }
    // Arrow `Decimal128(p,s)` / `Decimal256(p,s)` → `DECIMAL128` / `DECIMAL256`
    // after paren-strip.
    if normalized.starts_with("DECIMAL") {
        return TypeCategory::Numeric;
    }

    match normalized.as_str() {
        // Snowflake native
        "VARCHAR" | "CHAR" | "CHARACTER" | "STRING" | "TEXT" | "BINARY" | "VARBINARY" => {
            return TypeCategory::String;
        }
        // Arrow Display
        "UTF8" | "LARGEUTF8" | "LARGEBINARY" => return TypeCategory::String,
        _ => {}
    }

    TypeCategory::Other
}

/// Build a single SQL string that computes per-column stats for every column
/// in `column_names` plus the overall row count in one round-trip.
///
/// Emits, in order: `_col_idx` (u64), `_null_count`, `_distinct_count`,
/// `_min_val`, `_max_val`, `_mean`, `_std_dev`, `_total_count` — 8 columns per
/// row, one row per column, produced by UNION-ALLing one aggregate SELECT per
/// column against a shared CTE so the user's query is scanned / planned once.
/// `_total_count` is just `COUNT(*)` — each arm already aggregates over the
/// CTE without a GROUP BY, so the unfiltered `COUNT(*)` in the same SELECT
/// is the total row count.
///
/// Mean / std_dev expressions are routed through [`snowflake_type_category`]:
/// - Numeric columns use `CAST(<col> AS FLOAT)` (TRY_TO_DOUBLE would reject
///   non-VARCHAR input with `001065`).
/// - String columns use `TRY_TO_DOUBLE(<col>)` so non-numeric strings become
///   NULL and AVG/STDDEV ignore them.
/// - Everything else (dates, times, booleans, semi-structured, unknown) emits
///   `CAST(NULL AS FLOAT)` — computing a mean or std_dev on these is
///   meaningless.
///
/// Row order is not guaranteed by UNION ALL; callers must use `_col_idx` to
/// bucket rows back into the original column order.
///
/// Precondition: `column_names` is non-empty and `types.len() == column_names
/// .len()`. The caller short-circuits to a plain `SELECT COUNT(*)` when there
/// are no columns.
pub(super) fn build_multi_stat_sql(
    column_names: &[String],
    types: &[Option<&str>],
    inner_sql: &str,
) -> String {
    debug_assert!(!column_names.is_empty());
    debug_assert_eq!(column_names.len(), types.len());

    let selects: Vec<String> = column_names
        .iter()
        .zip(types.iter())
        .enumerate()
        .map(|(idx, (name, ty))| {
            let quoted = format!("\"{}\"", name.replace('"', "\"\""));
            let category = ty
                .map(snowflake_type_category)
                .unwrap_or(TypeCategory::Other);
            let (mean_expr, stddev_expr) = match category {
                TypeCategory::Numeric => (
                    format!("AVG(CAST({quoted} AS FLOAT))"),
                    format!("STDDEV_POP(CAST({quoted} AS FLOAT))"),
                ),
                TypeCategory::String => (
                    format!("AVG(TRY_TO_DOUBLE({quoted}))"),
                    format!("STDDEV_POP(TRY_TO_DOUBLE({quoted}))"),
                ),
                TypeCategory::Other => (
                    "CAST(NULL AS FLOAT)".to_string(),
                    "CAST(NULL AS FLOAT)".to_string(),
                ),
            };
            format!(
                "SELECT \
                    {idx} AS _col_idx, \
                    COUNT(*) - COUNT({quoted}) AS _null_count, \
                    COUNT(DISTINCT {quoted}) AS _distinct_count, \
                    MIN({quoted})::TEXT AS _min_val, \
                    MAX({quoted})::TEXT AS _max_val, \
                    {mean_expr} AS _mean, \
                    {stddev_expr} AS _std_dev, \
                    COUNT(*) AS _total_count \
                 FROM __oxy_stats_src"
            )
        })
        .collect();

    format!(
        "WITH __oxy_stats_src AS ({inner_sql}) {}",
        selects.join(" UNION ALL ")
    )
}

/// One decoded row from [`build_multi_stat_sql`].
#[derive(Debug)]
pub(super) struct StatRow {
    pub col_idx: u64,
    pub null_count: u64,
    pub distinct_count: u64,
    pub min: CellValue,
    pub max: CellValue,
    pub mean: Option<f64>,
    pub std_dev: Option<f64>,
    pub total: u64,
}

/// Decode every row produced by [`build_multi_stat_sql`]. Missing or malformed
/// cells degrade to sensible defaults; `decode_stat_rows` never fails.
pub(super) fn decode_stat_rows(result: SnowflakeQueryResult) -> Vec<StatRow> {
    let mut out: Vec<StatRow> = Vec::new();
    match result {
        SnowflakeQueryResult::Arrow(batches) => {
            for batch in batches.iter() {
                if batch.num_columns() < 8 {
                    continue;
                }
                for row_idx in 0..batch.num_rows() {
                    let as_u64 = |col: usize| -> u64 {
                        match arrow_to_cell(batch.column(col).as_ref(), row_idx) {
                            CellValue::Number(n) => n as u64,
                            _ => 0,
                        }
                    };
                    let as_f64 = |col: usize| -> Option<f64> {
                        match arrow_to_cell(batch.column(col).as_ref(), row_idx) {
                            CellValue::Number(n) => Some(n),
                            _ => None,
                        }
                    };
                    out.push(StatRow {
                        col_idx: as_u64(0),
                        null_count: as_u64(1),
                        distinct_count: as_u64(2),
                        min: arrow_to_cell(batch.column(3).as_ref(), row_idx),
                        max: arrow_to_cell(batch.column(4).as_ref(), row_idx),
                        mean: as_f64(5),
                        std_dev: as_f64(6),
                        total: as_u64(7),
                    });
                }
            }
        }
        SnowflakeQueryResult::Json(rows) => {
            if let Some(outer) = rows.value.as_array() {
                for row in outer {
                    let Some(vals) = row.as_array() else { continue };
                    if vals.len() < 8 {
                        continue;
                    }
                    let get_u64 = |i: usize| -> u64 {
                        match vals.get(i) {
                            Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(0),
                            Some(serde_json::Value::String(s)) => s.parse().unwrap_or(0),
                            _ => 0,
                        }
                    };
                    let get_f64 = |i: usize| -> Option<f64> {
                        match vals.get(i) {
                            Some(serde_json::Value::Number(n)) => n.as_f64(),
                            Some(serde_json::Value::String(s)) => s.parse().ok(),
                            _ => None,
                        }
                    };
                    let get_cell = |i: usize| -> CellValue {
                        vals.get(i)
                            .map(json_value_to_cell)
                            .unwrap_or(CellValue::Null)
                    };
                    out.push(StatRow {
                        col_idx: get_u64(0),
                        null_count: get_u64(1),
                        distinct_count: get_u64(2),
                        min: get_cell(3),
                        max: get_cell(4),
                        mean: get_f64(5),
                        std_dev: get_f64(6),
                        total: get_u64(7),
                    });
                }
            }
        }
        SnowflakeQueryResult::Empty => {}
    }
    out
}

// ── Schema pre-fetch ──────────────────────────────────────────────────────────

/// Fetch the schema from Snowflake's INFORMATION_SCHEMA.COLUMNS.
#[allow(clippy::too_many_arguments)]
pub(super) async fn fetch_schema(
    account: &str,
    username: &str,
    auth: &SnowflakeAuth,
    role: Option<&str>,
    warehouse: &str,
    database: Option<&str>,
    schema_str: Option<&str>,
) -> Result<SchemaInfo, ConnectorError> {
    let api = build_api(
        account, username, auth, role, warehouse, database, schema_str,
    )?;
    api.authenticate()
        .await
        .map_err(|e| ConnectorError::ConnectionError(e.to_string()))?;

    let schema_sql = match database {
        Some(db) => format!(
            "SELECT TABLE_NAME, COLUMN_NAME, DATA_TYPE \
             FROM {db}.INFORMATION_SCHEMA.COLUMNS \
             WHERE TABLE_SCHEMA NOT IN ('INFORMATION_SCHEMA') \
             ORDER BY TABLE_NAME, ORDINAL_POSITION"
        ),
        None => "\
            SELECT TABLE_NAME, COLUMN_NAME, DATA_TYPE \
            FROM INFORMATION_SCHEMA.COLUMNS \
            WHERE TABLE_SCHEMA NOT IN ('INFORMATION_SCHEMA') \
            ORDER BY TABLE_NAME, ORDINAL_POSITION"
            .to_string(),
    };

    let result = api
        .exec(&schema_sql)
        .await
        .map_err(|e| ConnectorError::QueryFailed {
            sql: schema_sql.clone(),
            message: e.to_string(),
        })?;

    let mut map: HashMap<String, Vec<SchemaColumnInfo>> = HashMap::new();

    match result {
        SnowflakeQueryResult::Arrow(batches) => {
            for batch in &batches {
                if batch.num_columns() < 3 {
                    continue;
                }
                for row in 0..batch.num_rows() {
                    let table = match arrow_to_cell(batch.column(0).as_ref(), row) {
                        CellValue::Text(s) => s,
                        _ => continue,
                    };
                    let column = match arrow_to_cell(batch.column(1).as_ref(), row) {
                        CellValue::Text(s) => s,
                        _ => continue,
                    };
                    let data_type = match arrow_to_cell(batch.column(2).as_ref(), row) {
                        CellValue::Text(s) => s,
                        _ => String::new(),
                    };
                    map.entry(table).or_default().push(SchemaColumnInfo {
                        name: column,
                        data_type,
                        min: None,
                        max: None,
                        sample_values: vec![],
                    });
                }
            }
        }
        SnowflakeQueryResult::Json(rows) => {
            // JsonResult.value is array-of-arrays; each inner array is [table, column, type]
            if let Some(outer) = rows.value.as_array() {
                for row in outer {
                    let Some(vals) = row.as_array() else { continue };
                    let table = match vals.first() {
                        Some(serde_json::Value::String(s)) => s.clone(),
                        _ => continue,
                    };
                    let column = match vals.get(1) {
                        Some(serde_json::Value::String(s)) => s.clone(),
                        _ => continue,
                    };
                    let data_type = match vals.get(2) {
                        Some(serde_json::Value::String(s)) => s.clone(),
                        _ => String::new(),
                    };
                    map.entry(table).or_default().push(SchemaColumnInfo {
                        name: column,
                        data_type,
                        min: None,
                        max: None,
                        sample_values: vec![],
                    });
                }
            }
        }
        SnowflakeQueryResult::Empty => {}
    }

    let tables: Vec<SchemaTableInfo> = map
        .into_iter()
        .map(|(name, columns)| SchemaTableInfo { name, columns })
        .collect();

    let join_keys = detect_join_keys(&tables);
    Ok(SchemaInfo { tables, join_keys })
}

// ── Join key detection ────────────────────────────────────────────────────────

/// Auto-detect join keys: any column ending in `_id` shared across two tables.
pub(super) fn detect_join_keys(tables: &[SchemaTableInfo]) -> Vec<(String, String, String)> {
    let mut col_to_tables: HashMap<&str, Vec<&str>> = HashMap::new();
    for t in tables {
        for c in &t.columns {
            if c.name.ends_with("_id") {
                col_to_tables
                    .entry(c.name.as_str())
                    .or_default()
                    .push(t.name.as_str());
            }
        }
    }
    let mut keys = Vec::new();
    for (col, tbs) in col_to_tables {
        for i in 0..tbs.len() {
            for j in (i + 1)..tbs.len() {
                keys.push((tbs[i].to_string(), tbs[j].to_string(), col.to_string()));
            }
        }
    }
    keys
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_category_snowflake_numerics() {
        assert_eq!(
            snowflake_type_category("NUMBER(18,0)"),
            TypeCategory::Numeric
        );
        assert_eq!(
            snowflake_type_category("DECIMAL(10,2)"),
            TypeCategory::Numeric
        );
        assert_eq!(snowflake_type_category("NUMERIC"), TypeCategory::Numeric);
        assert_eq!(snowflake_type_category("INT"), TypeCategory::Numeric);
        assert_eq!(snowflake_type_category("INTEGER"), TypeCategory::Numeric);
        assert_eq!(snowflake_type_category("BIGINT"), TypeCategory::Numeric);
        assert_eq!(snowflake_type_category("SMALLINT"), TypeCategory::Numeric);
        assert_eq!(snowflake_type_category("TINYINT"), TypeCategory::Numeric);
        assert_eq!(snowflake_type_category("BYTEINT"), TypeCategory::Numeric);
        assert_eq!(snowflake_type_category("FLOAT"), TypeCategory::Numeric);
        assert_eq!(snowflake_type_category("FLOAT4"), TypeCategory::Numeric);
        assert_eq!(snowflake_type_category("FLOAT8"), TypeCategory::Numeric);
        assert_eq!(snowflake_type_category("DOUBLE"), TypeCategory::Numeric);
        assert_eq!(
            snowflake_type_category("DOUBLE PRECISION"),
            TypeCategory::Numeric
        );
        assert_eq!(snowflake_type_category("REAL"), TypeCategory::Numeric);
    }

    #[test]
    fn type_category_arrow_numerics() {
        assert_eq!(snowflake_type_category("Int64"), TypeCategory::Numeric);
        assert_eq!(snowflake_type_category("Int32"), TypeCategory::Numeric);
        assert_eq!(snowflake_type_category("UInt16"), TypeCategory::Numeric);
        assert_eq!(snowflake_type_category("Float64"), TypeCategory::Numeric);
        assert_eq!(snowflake_type_category("Float32"), TypeCategory::Numeric);
        assert_eq!(
            snowflake_type_category("Decimal128(38, 0)"),
            TypeCategory::Numeric
        );
        assert_eq!(
            snowflake_type_category("Decimal256(38, 10)"),
            TypeCategory::Numeric
        );
    }

    #[test]
    fn type_category_snowflake_strings() {
        assert_eq!(
            snowflake_type_category("VARCHAR(16777216)"),
            TypeCategory::String
        );
        assert_eq!(snowflake_type_category("VARCHAR"), TypeCategory::String);
        assert_eq!(snowflake_type_category("CHAR(10)"), TypeCategory::String);
        assert_eq!(snowflake_type_category("CHARACTER"), TypeCategory::String);
        assert_eq!(snowflake_type_category("STRING"), TypeCategory::String);
        assert_eq!(snowflake_type_category("TEXT"), TypeCategory::String);
        assert_eq!(snowflake_type_category("BINARY"), TypeCategory::String);
        assert_eq!(snowflake_type_category("VARBINARY"), TypeCategory::String);
    }

    #[test]
    fn type_category_arrow_strings() {
        assert_eq!(snowflake_type_category("Utf8"), TypeCategory::String);
        assert_eq!(snowflake_type_category("LargeUtf8"), TypeCategory::String);
        assert_eq!(snowflake_type_category("LargeBinary"), TypeCategory::String);
    }

    #[test]
    fn type_category_other() {
        assert_eq!(
            snowflake_type_category("TIMESTAMP_NTZ(9)"),
            TypeCategory::Other
        );
        assert_eq!(
            snowflake_type_category("TIMESTAMP_LTZ(9)"),
            TypeCategory::Other
        );
        assert_eq!(snowflake_type_category("TIMESTAMP"), TypeCategory::Other);
        assert_eq!(snowflake_type_category("DATE"), TypeCategory::Other);
        assert_eq!(snowflake_type_category("TIME"), TypeCategory::Other);
        assert_eq!(snowflake_type_category("BOOLEAN"), TypeCategory::Other);
        assert_eq!(snowflake_type_category("VARIANT"), TypeCategory::Other);
        assert_eq!(snowflake_type_category("OBJECT"), TypeCategory::Other);
        assert_eq!(snowflake_type_category("ARRAY"), TypeCategory::Other);
        assert_eq!(snowflake_type_category("GEOGRAPHY"), TypeCategory::Other);
        assert_eq!(snowflake_type_category("GEOMETRY"), TypeCategory::Other);
        // Arrow forms.
        assert_eq!(snowflake_type_category("Date32"), TypeCategory::Other);
        assert_eq!(
            snowflake_type_category("Timestamp(Microsecond, None)"),
            TypeCategory::Other
        );
    }

    #[test]
    fn type_category_case_and_whitespace() {
        assert_eq!(
            snowflake_type_category("  number(18,0)  "),
            TypeCategory::Numeric
        );
        assert_eq!(snowflake_type_category("varchar"), TypeCategory::String);
        assert_eq!(
            snowflake_type_category("timestamp_ntz(9)"),
            TypeCategory::Other
        );
    }

    #[test]
    fn multi_stat_sql_numeric_uses_cast_to_float() {
        let sql = build_multi_stat_sql(
            &["col_a".to_string()],
            &[Some("NUMBER(18,0)")],
            "SELECT * FROM t",
        );
        assert!(
            sql.contains("AVG(CAST(\"col_a\" AS FLOAT))"),
            "sql was: {sql}"
        );
        assert!(
            sql.contains("STDDEV_POP(CAST(\"col_a\" AS FLOAT))"),
            "sql was: {sql}"
        );
        assert!(!sql.contains("TRY_TO_DOUBLE"), "sql was: {sql}");
    }

    #[test]
    fn multi_stat_sql_string_uses_try_to_double() {
        let sql = build_multi_stat_sql(
            &["col_b".to_string()],
            &[Some("VARCHAR(100)")],
            "SELECT * FROM t",
        );
        assert!(
            sql.contains("AVG(TRY_TO_DOUBLE(\"col_b\"))"),
            "sql was: {sql}"
        );
        assert!(
            sql.contains("STDDEV_POP(TRY_TO_DOUBLE(\"col_b\"))"),
            "sql was: {sql}"
        );
    }

    #[test]
    fn multi_stat_sql_other_emits_null() {
        let sql = build_multi_stat_sql(
            &["col_c".to_string()],
            &[Some("TIMESTAMP_NTZ(9)")],
            "SELECT * FROM t",
        );
        // Two NULLs — one for mean, one for std_dev.
        assert_eq!(
            sql.matches("CAST(NULL AS FLOAT)").count(),
            2,
            "sql was: {sql}"
        );
        assert!(!sql.contains("AVG(CAST(\"col_c\""), "sql was: {sql}");
        assert!(!sql.contains("TRY_TO_DOUBLE"), "sql was: {sql}");
    }

    #[test]
    fn multi_stat_sql_missing_type_defaults_to_other() {
        let sql = build_multi_stat_sql(&["col_d".to_string()], &[None], "SELECT * FROM t");
        assert_eq!(
            sql.matches("CAST(NULL AS FLOAT)").count(),
            2,
            "sql was: {sql}"
        );
    }

    #[test]
    fn multi_stat_sql_projection_alias_order() {
        let sql = build_multi_stat_sql(&["c".to_string()], &[Some("INT")], "SELECT c FROM t");
        // decode_stat_rows reads columns by index; assert the alias ordering.
        let positions = [
            sql.find("_col_idx"),
            sql.find("_null_count"),
            sql.find("_distinct_count"),
            sql.find("_min_val"),
            sql.find("_max_val"),
            sql.find("_mean"),
            sql.find("_std_dev"),
            sql.find("_total_count"),
        ];
        assert!(
            positions.iter().all(|p| p.is_some()),
            "all 8 aliases must appear, sql was: {sql}"
        );
        let positions: Vec<usize> = positions.iter().map(|p| p.unwrap()).collect();
        for pair in positions.windows(2) {
            assert!(
                pair[0] < pair[1],
                "alias order broken at {pair:?}, sql was: {sql}"
            );
        }
    }

    #[test]
    fn multi_stat_sql_escapes_embedded_double_quotes() {
        let sql = build_multi_stat_sql(
            &["weird\"name".to_string()],
            &[Some("INT")],
            "SELECT x FROM t",
        );
        assert!(sql.contains("\"weird\"\"name\""), "sql was: {sql}");
    }

    #[test]
    fn multi_stat_sql_mixed_columns_from_task_fixture() {
        // Exactly the fixture the reviewer specified — now packed into one
        // UNION ALL instead of three separate queries.
        let names = [
            "col_a".to_string(),
            "col_b".to_string(),
            "col_c".to_string(),
        ];
        let types = [
            Some("NUMBER(18,0)"),
            Some("VARCHAR(100)"),
            Some("TIMESTAMP_NTZ(9)"),
        ];
        let sql = build_multi_stat_sql(&names, &types, "SELECT * FROM t");

        assert!(sql.contains("AVG(CAST(\"col_a\" AS FLOAT))"));
        assert!(sql.contains("AVG(TRY_TO_DOUBLE(\"col_b\"))"));
        // col_c → Other → NULLs for mean/std_dev. Plus col_a / col_b don't
        // emit CAST(NULL AS FLOAT), so only the 2 from col_c appear.
        assert_eq!(sql.matches("CAST(NULL AS FLOAT)").count(), 2);
        // One UNION ALL per pair of selects → 2 for 3 columns.
        assert_eq!(sql.matches("UNION ALL").count(), 2);
    }

    #[test]
    fn multi_stat_sql_cte_wraps_inner_once() {
        let sql = build_multi_stat_sql(
            &["a".to_string(), "b".to_string()],
            &[Some("INT"), Some("VARCHAR")],
            "SELECT sentinel_column_name_xyz FROM sentinel_table_name_xyz",
        );
        // CTE wraps the inner SQL exactly once.
        assert_eq!(
            sql.matches("WITH __oxy_stats_src AS (SELECT sentinel_column_name_xyz FROM sentinel_table_name_xyz)")
                .count(),
            1
        );
        // The inner SQL never appears outside the CTE — every branch
        // references `__oxy_stats_src`.
        assert_eq!(sql.matches("sentinel_column_name_xyz").count(), 1);
        // Every SELECT carries `COUNT(*) AS _total_count` directly (no
        // scalar subquery — each arm already aggregates over the CTE).
        assert_eq!(sql.matches("COUNT(*) AS _total_count").count(), 2);
    }
}
