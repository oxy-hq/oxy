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
    Array, BooleanArray, Date32Array, Decimal128Array, Float32Array, Float64Array, Int16Array,
    Int32Array, Int64Array, Int8Array, LargeStringArray, StringArray, UInt16Array, UInt32Array,
    UInt64Array, UInt8Array,
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
        let mut api = SnowflakeApi::with_password_auth(
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
        let mut api = SnowflakeApi::with_password_auth(
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

        // Per-column stats via a separate query on the subquery.
        let mut col_stats: Vec<ColumnStats> = Vec::with_capacity(col_count);
        for (idx, col) in column_names.iter().enumerate() {
            let quoted = format!("\"{}\"", col.replace('"', "\"\""));

            let stat_sql = format!(
                "SELECT \
                    COUNT(*) - COUNT({quoted}), \
                    COUNT(DISTINCT {quoted}), \
                    MIN({quoted})::TEXT, \
                    MAX({quoted})::TEXT, \
                    AVG(TRY_TO_DOUBLE({quoted})), \
                    STDDEV_POP(TRY_TO_DOUBLE({quoted})) \
                 FROM ({sql})"
            );

            let stat_api = self.connect().await?;
            let stat_result =
                stat_api
                    .exec(&stat_sql)
                    .await
                    .map_err(|e| ConnectorError::QueryFailed {
                        sql: stat_sql.clone(),
                        message: e.to_string(),
                    })?;

            let (null_count, distinct_count, min_v, max_v, mean, std_dev) =
                extract_stat_row(stat_result);

            col_stats.push(ColumnStats {
                name: col.clone(),
                data_type: column_types.get(idx).cloned().flatten(),
                null_count,
                distinct_count: Some(distinct_count),
                min: Some(min_v),
                max: Some(max_v),
                mean,
                std_dev,
            });
        }

        // Total row count via COUNT(*).
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
        let total_row_count = extract_count(count_result);

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

/// Extract the 6-column stats row (null_count, distinct_count, min, max, mean, std_dev).
fn extract_stat_row(
    result: SnowflakeQueryResult,
) -> (u64, u64, CellValue, CellValue, Option<f64>, Option<f64>) {
    let default = (0u64, 0u64, CellValue::Null, CellValue::Null, None, None);
    match result {
        SnowflakeQueryResult::Arrow(batches) => {
            let Some(batch) = batches.first() else {
                return default;
            };
            if batch.num_rows() == 0 || batch.num_columns() < 4 {
                return default;
            }
            let null_count = match arrow_to_cell(batch.column(0).as_ref(), 0) {
                CellValue::Number(n) => n as u64,
                _ => 0,
            };
            let distinct_count = match arrow_to_cell(batch.column(1).as_ref(), 0) {
                CellValue::Number(n) => n as u64,
                _ => 0,
            };
            let min_v = arrow_to_cell(batch.column(2).as_ref(), 0);
            let max_v = arrow_to_cell(batch.column(3).as_ref(), 0);
            let mean = if batch.num_columns() > 4 {
                match arrow_to_cell(batch.column(4).as_ref(), 0) {
                    CellValue::Number(n) => Some(n),
                    _ => None,
                }
            } else {
                None
            };
            let std_dev = if batch.num_columns() > 5 {
                match arrow_to_cell(batch.column(5).as_ref(), 0) {
                    CellValue::Number(n) => Some(n),
                    _ => None,
                }
            } else {
                None
            };
            (null_count, distinct_count, min_v, max_v, mean, std_dev)
        }
        SnowflakeQueryResult::Json(rows) => {
            let Some(vals) = rows
                .value
                .as_array()
                .and_then(|outer| outer.first())
                .and_then(|row| row.as_array())
            else {
                return default;
            };
            let vals: Vec<&serde_json::Value> = vals.iter().collect();
            let get_u64 = |v: Option<&&serde_json::Value>| -> u64 {
                v.and_then(|v| match v {
                    serde_json::Value::Number(n) => n.as_u64(),
                    serde_json::Value::String(s) => s.parse().ok(),
                    _ => None,
                })
                .unwrap_or(0)
            };
            let get_f64 = |v: Option<&&serde_json::Value>| -> Option<f64> {
                v.and_then(|v| match v {
                    serde_json::Value::Number(n) => n.as_f64(),
                    serde_json::Value::String(s) => s.parse().ok(),
                    _ => None,
                })
            };
            let null_count = get_u64(vals.first());
            let distinct_count = get_u64(vals.get(1));
            let min_v = vals
                .get(2)
                .map(|v| json_value_to_cell(v))
                .unwrap_or(CellValue::Null);
            let max_v = vals
                .get(3)
                .map(|v| json_value_to_cell(v))
                .unwrap_or(CellValue::Null);
            let mean = get_f64(vals.get(4));
            let std_dev = get_f64(vals.get(5));
            (null_count, distinct_count, min_v, max_v, mean, std_dev)
        }
        SnowflakeQueryResult::Empty => default,
    }
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
    let mut api = SnowflakeApi::with_password_auth(
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
