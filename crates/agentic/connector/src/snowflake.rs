//! Snowflake connector implementation.
//!
//! Uses `snowflake_api::SnowflakeApi` for authentication and query execution.
//! Because `SnowflakeApi` is not `Sync`, a new API instance is created per
//! query via the `connect()` helper.
//!
//! Snowflake returns results in Arrow format (`QueryResult::Arrow`) or JSON
//! (`QueryResult::Json`).  Arrow batches are decoded using the
//! `arrow_to_cell` helper which handles all common Arrow types.
//!
//! Schema is introspected from `INFORMATION_SCHEMA.COLUMNS` and cached at
//! construction time because `introspect_schema()` is synchronous.

#![cfg(feature = "snowflake")]

use std::collections::HashMap;

use arrow::array::{
    Array, BooleanArray, Date32Array, Decimal128Array, Float32Array, Float64Array, Int8Array,
    Int16Array, Int32Array, Int64Array, LargeStringArray, StringArray, UInt8Array, UInt16Array,
    UInt32Array, UInt64Array,
};
use arrow::datatypes::DataType;
use async_trait::async_trait;
use snowflake_api::{QueryResult as SnowflakeQueryResult, SnowflakeApi};

use agentic_core::result::{CellValue, QueryResult, QueryRow};

use crate::connector::{
    ColumnStats, ConnectorError, DatabaseConnector, ExecutionResult, ResultSummary,
    SchemaColumnInfo, SchemaInfo, SchemaTableInfo, SqlDialect,
};

// ── Arrow → CellValue ─────────────────────────────────────────────────────────

/// Convert a single cell from an Arrow column array into a [`CellValue`].
fn arrow_to_cell(array: &dyn Array, row: usize) -> CellValue {
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
/// query because the API client is not `Sync`.
pub struct SnowflakeConnector {
    account: String,
    username: String,
    password: String,
    role: Option<String>,
    warehouse: String,
    database: Option<String>,
    schema_str: Option<String>,
    cached_schema: SchemaInfo,
}

impl SnowflakeConnector {
    /// Create a new connector and pre-fetch the database schema.
    pub async fn new(
        account: String,
        username: String,
        password: String,
        role: Option<String>,
        warehouse: String,
        database: Option<String>,
        schema_str: Option<String>,
    ) -> Result<Self, ConnectorError> {
        // Authenticate once to validate credentials.
        let api = SnowflakeApi::with_password_auth(
            &account,
            Some(&warehouse),
            database.as_deref(),
            schema_str.as_deref(),
            &username,
            role.as_deref(),
            &password,
        )
        .map_err(|e| ConnectorError::ConnectionError(e.to_string()))?;

        api.authenticate()
            .await
            .map_err(|e| ConnectorError::ConnectionError(e.to_string()))?;

        let cached_schema = fetch_schema(
            &account,
            &username,
            &password,
            role.as_deref(),
            &warehouse,
            database.as_deref(),
            schema_str.as_deref(),
        )
        .await
        .unwrap_or_default();

        Ok(Self {
            account,
            username,
            password,
            role,
            warehouse,
            database,
            schema_str,
            cached_schema,
        })
    }

    /// Create and authenticate a fresh [`SnowflakeApi`] for a single query.
    async fn connect(&self) -> Result<SnowflakeApi, ConnectorError> {
        let api = SnowflakeApi::with_password_auth(
            &self.account,
            Some(&self.warehouse),
            self.database.as_deref(),
            self.schema_str.as_deref(),
            &self.username,
            self.role.as_deref(),
            &self.password,
        )
        .map_err(|e| ConnectorError::ConnectionError(e.to_string()))?;

        api.authenticate()
            .await
            .map_err(|e| ConnectorError::ConnectionError(e.to_string()))?;

        Ok(api)
    }
}

// ── DatabaseConnector impl ────────────────────────────────────────────────────

#[async_trait]
impl DatabaseConnector for SnowflakeConnector {
    fn dialect(&self) -> SqlDialect {
        SqlDialect::Snowflake
    }

    async fn execute_query(
        &self,
        sql: &str,
        sample_limit: u64,
    ) -> Result<ExecutionResult, ConnectorError> {
        let api = self.connect().await?;

        let sf_result = api
            .exec(sql)
            .await
            .map_err(|e| ConnectorError::QueryFailed {
                sql: sql.to_string(),
                message: e.to_string(),
            })?;

        // Decode rows and column names from the Snowflake result.
        let (column_names, column_types, mut sample_rows) = match sf_result {
            SnowflakeQueryResult::Arrow(batches) => {
                let (columns, types) = batches
                    .first()
                    .map(|b| {
                        let cols = b
                            .schema()
                            .fields()
                            .iter()
                            .map(|f| f.name().clone())
                            .collect::<Vec<_>>();
                        let tys = b
                            .schema()
                            .fields()
                            .iter()
                            .map(|f| Some(format!("{}", f.data_type())))
                            .collect::<Vec<_>>();
                        (cols, tys)
                    })
                    .unwrap_or_default();

                let mut rows: Vec<QueryRow> = Vec::new();
                for batch in &batches {
                    for row_idx in 0..batch.num_rows() {
                        let cells = (0..batch.num_columns())
                            .map(|col_idx| arrow_to_cell(batch.column(col_idx).as_ref(), row_idx))
                            .collect();
                        rows.push(QueryRow(cells));
                    }
                }
                (columns, types, rows)
            }
            SnowflakeQueryResult::Json(json_rows) => {
                // JsonResult.value is an array-of-arrays: [[v1, v2], [v3, v4], ...]
                // JsonResult.schema is Vec<FieldSchema> with column names.
                let columns: Vec<String> =
                    json_rows.schema.iter().map(|f| f.name.clone()).collect();
                let types: Vec<Option<String>> = vec![None; columns.len()];

                let rows = json_rows
                    .value
                    .as_array()
                    .map(|outer| {
                        outer
                            .iter()
                            .filter_map(|r| r.as_array())
                            .map(|vals| {
                                let cells = vals.iter().map(json_value_to_cell).collect();
                                QueryRow(cells)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                (columns, types, rows)
            }
            SnowflakeQueryResult::Empty => (Vec::new(), Vec::new(), Vec::new()),
        };

        // Apply sample limit.
        sample_rows.truncate(sample_limit as usize);

        let col_count = column_names.len();

        // Per-column stats + total row count via a single round-trip.
        // Previously this was one `connect()` + `exec()` per column — 20 columns
        // meant 20 sequential authentication round-trips to Snowflake, plus a
        // 21st for the total row count. The new query wraps the user's SQL in a
        // CTE (so the source is scanned / planned once), UNION ALLs one
        // aggregate SELECT per column, and carries the total row count on every
        // row via a scalar subquery.
        let col_stats: Vec<ColumnStats>;
        let total_row_count: u64;

        if col_count == 0 {
            col_stats = Vec::new();
            let count_sql = format!("SELECT COUNT(*) FROM ({sql})");
            let count_api = self.connect().await?;
            let count_result =
                count_api
                    .exec(&count_sql)
                    .await
                    .map_err(|e| ConnectorError::QueryFailed {
                        sql: count_sql.clone(),
                        message: e.to_string(),
                    })?;
            total_row_count = extract_count(count_result);
        } else {
            let types: Vec<Option<&str>> = column_names
                .iter()
                .enumerate()
                .map(|(idx, _)| column_types.get(idx).and_then(|t| t.as_deref()))
                .collect();
            let stats_sql = build_multi_stat_sql(&column_names, &types, sql);
            let stats_api = self.connect().await?;
            let stats_result =
                stats_api
                    .exec(&stats_sql)
                    .await
                    .map_err(|e| ConnectorError::QueryFailed {
                        sql: stats_sql.clone(),
                        message: e.to_string(),
                    })?;

            let mut slots: Vec<Option<StatRow>> = (0..col_count).map(|_| None).collect();
            let mut total_from_stats: u64 = 0;
            for row in decode_stat_rows(stats_result) {
                total_from_stats = row.total;
                let idx = row.col_idx as usize;
                if idx < col_count {
                    slots[idx] = Some(row);
                }
            }
            total_row_count = total_from_stats;

            col_stats = column_names
                .iter()
                .enumerate()
                .map(|(idx, col)| {
                    let row = slots[idx].take();
                    ColumnStats {
                        name: col.clone(),
                        data_type: column_types.get(idx).cloned().flatten(),
                        null_count: row.as_ref().map(|r| r.null_count).unwrap_or(0),
                        distinct_count: row.as_ref().map(|r| r.distinct_count),
                        min: Some(row.as_ref().map(|r| r.min.clone()).unwrap_or(CellValue::Null)),
                        max: Some(row.as_ref().map(|r| r.max.clone()).unwrap_or(CellValue::Null)),
                        mean: row.as_ref().and_then(|r| r.mean),
                        std_dev: row.as_ref().and_then(|r| r.std_dev),
                    }
                })
                .collect();
        }

        let truncated = (sample_rows.len() as u64) < total_row_count;
        Ok(ExecutionResult {
            result: QueryResult {
                columns: column_names,
                rows: sample_rows,
                total_row_count,
                truncated,
            },
            summary: ResultSummary {
                row_count: total_row_count,
                columns: col_stats,
            },
        })
    }

    fn introspect_schema(&self) -> Result<SchemaInfo, ConnectorError> {
        Ok(self.cached_schema.clone())
    }
}

// ── Helper functions ──────────────────────────────────────────────────────────

/// Convert a `serde_json::Value` to a [`CellValue`].
fn json_value_to_cell(v: &serde_json::Value) -> CellValue {
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

/// Extract a scalar COUNT from a Snowflake query result.
fn extract_count(result: SnowflakeQueryResult) -> u64 {
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
enum TypeCategory {
    /// Numeric types — AVG/STDDEV can read the column directly.
    Numeric,
    /// String/binary types — must be parsed to double first.
    String,
    /// Everything else (dates, times, booleans, semi-structured, or unknown).
    /// Mean / std_dev are emitted as NULL.
    Other,
}

/// Normalize a type string and bucket it into [`TypeCategory`].
fn snowflake_type_category(raw: &str) -> TypeCategory {
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
        | "TINYINT" | "BYTEINT" | "FLOAT" | "FLOAT4" | "FLOAT8" | "DOUBLE"
        | "DOUBLE PRECISION" | "REAL" => return TypeCategory::Numeric,
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
fn build_multi_stat_sql(
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
            let category = ty.map(|t| snowflake_type_category(t)).unwrap_or(TypeCategory::Other);
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
struct StatRow {
    col_idx: u64,
    null_count: u64,
    distinct_count: u64,
    min: CellValue,
    max: CellValue,
    mean: Option<f64>,
    std_dev: Option<f64>,
    total: u64,
}

/// Decode every row produced by [`build_multi_stat_sql`]. Missing or malformed
/// cells degrade to sensible defaults; `decode_stat_rows` never fails.
fn decode_stat_rows(result: SnowflakeQueryResult) -> Vec<StatRow> {
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
                        vals.get(i).map(json_value_to_cell).unwrap_or(CellValue::Null)
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
async fn fetch_schema(
    account: &str,
    username: &str,
    password: &str,
    role: Option<&str>,
    warehouse: &str,
    database: Option<&str>,
    schema_str: Option<&str>,
) -> Result<SchemaInfo, ConnectorError> {
    let api = SnowflakeApi::with_password_auth(
        account,
        Some(warehouse),
        database,
        schema_str,
        username,
        role,
        password,
    )
    .map_err(|e| ConnectorError::ConnectionError(e.to_string()))?;

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
fn detect_join_keys(tables: &[SchemaTableInfo]) -> Vec<(String, String, String)> {
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
        assert_eq!(
            snowflake_type_category("LargeBinary"),
            TypeCategory::String
        );
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
        assert_eq!(
            snowflake_type_category("varchar"),
            TypeCategory::String
        );
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
