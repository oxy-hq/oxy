//! ClickHouse connector implementation via the HTTP API.
//!
//! ClickHouse exposes an HTTP interface (default port 8123).  Each query is
//! a POST request with the SQL in the body and ` FORMAT JSONCompact` appended.
//! Responses look like:
//!
//! ```json
//! {"meta":[{"name":"col","type":"Int64"}],"data":[[1],[2]],"rows":2}
//! ```
//!
//! Because ClickHouse does not support ANSI temporary tables, all temp-table
//! operations are replaced by subqueries:
//!
//! - Count:  `SELECT count() FROM ({sql})`
//! - Sample: `SELECT * FROM ({sql}) LIMIT {n} FORMAT JSONCompact`
//! - Stats:  per-column aggregation inside `FROM ({sql})`
//!
//! Schema is introspected from `system.columns` and cached at construction.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

use agentic_core::result::{
    CellValue, ColumnSpec, QueryResult, QueryRow, TypedRowError, TypedRowStream, TypedValue,
};

use crate::clickhouse_typed::{ch_type_to_typed, parse_ch_cell};
use crate::connector::{
    ColumnStats, ConnectorError, DatabaseConnector, ExecutionResult, ResultSummary,
    SchemaColumnInfo, SchemaInfo, SchemaTableInfo, SqlDialect, normalize_sql,
};

// ── HTTP response types ────────────────────────────────────────────────────────

/// Parsed ClickHouse JSONCompact response.
#[derive(Debug, Deserialize)]
struct ChResponse {
    meta: Vec<ChMeta>,
    data: Vec<Vec<Value>>,
    #[allow(dead_code)]
    #[serde(default)]
    rows: u64,
}

#[derive(Debug, Deserialize)]
struct ChMeta {
    name: String,
    #[serde(default)]
    r#type: Option<String>,
}

// ── Type classification ────────────────────────────────────────────────────────

/// Broad category used to pick the right avg/stddev expression for a column.
///
/// ClickHouse's `toFloat64OrNull` only accepts `String` input. Wrapping an
/// already-numeric column (e.g. the result of `SUM(...)`) produces:
/// `Illegal type Float64 of first argument of function toFloat64OrNull`.
/// So numeric columns must use `avg` / `stddevPop` directly.
enum TypeCategory {
    /// Numeric types — avg/stddevPop can read the column directly.
    Numeric,
    /// String types — use toFloat64OrNull so non-numeric strings become NULL.
    String,
    /// Everything else (Date, DateTime, UUID, …) — skip mean/stddev.
    Other,
}

/// Classify a ClickHouse column type string into a [`TypeCategory`].
///
/// Handles bare types (`Float64`, `Int32`) plus `Nullable(...)` and
/// `LowCardinality(...)` wrappers, including arbitrary nesting like
/// `LowCardinality(Nullable(String))` or `Nullable(LowCardinality(FixedString(16)))`.
fn clickhouse_type_category(raw: &str) -> TypeCategory {
    // Repeatedly strip Nullable(...) and LowCardinality(...) wrappers until the
    // inner type stands alone. ClickHouse allows them in either order.
    let mut inner = raw.trim();
    loop {
        let stripped = inner
            .strip_prefix("Nullable(")
            .or_else(|| inner.strip_prefix("LowCardinality("))
            .and_then(|s| s.strip_suffix(')'));
        match stripped {
            Some(s) => inner = s.trim(),
            None => break,
        }
    }

    // Strip parenthesized precision/scale (e.g. "Decimal(18, 4)" → "Decimal",
    // "FixedString(16)" → "FixedString").
    let base = inner.split('(').next().unwrap_or(inner).trim();

    match base {
        "Float32" | "Float64" | "Int8" | "Int16" | "Int32" | "Int64" | "Int128" | "Int256"
        | "UInt8" | "UInt16" | "UInt32" | "UInt64" | "UInt128" | "UInt256" | "Decimal"
        | "Decimal32" | "Decimal64" | "Decimal128" | "Decimal256" => TypeCategory::Numeric,
        "String" | "FixedString" => TypeCategory::String,
        _ => TypeCategory::Other,
    }
}

// ── Value converter ────────────────────────────────────────────────────────────

/// Convert a `serde_json::Value` cell from a JSONCompact row into a [`CellValue`].
fn json_to_cell(v: &Value) -> CellValue {
    match v {
        Value::Null => CellValue::Null,
        Value::Number(n) => CellValue::Number(n.as_f64().unwrap_or(0.0)),
        Value::Bool(b) => CellValue::Number(if *b { 1.0 } else { 0.0 }),
        Value::String(s) => {
            // ClickHouse returns numbers as strings for many types.
            if let Ok(n) = s.parse::<f64>() {
                CellValue::Number(n)
            } else {
                CellValue::Text(s.clone())
            }
        }
        other => CellValue::Text(other.to_string()),
    }
}

// ── Connector ─────────────────────────────────────────────────────────────────

/// ClickHouse connector that speaks the HTTP JSON API.
pub struct ClickHouseConnector {
    client: reqwest::Client,
    url: String,
    user: String,
    password: String,
    database: String,
    cached_schema: SchemaInfo,
}

impl ClickHouseConnector {
    /// Connect to ClickHouse via its HTTP interface and pre-fetch the schema.
    ///
    /// `url` should be the base URL including scheme and port, e.g.
    /// `http://localhost:8123`.
    pub async fn new(
        url: String,
        user: String,
        password: String,
        database: String,
    ) -> Result<Self, ConnectorError> {
        let client = reqwest::Client::new();
        let cached_schema = fetch_schema(&client, &url, &user, &password, &database).await?;

        Ok(Self {
            client,
            url,
            user,
            password,
            database,
            cached_schema,
        })
    }

    /// Execute a SQL string against ClickHouse via HTTP, returning the parsed
    /// JSONCompact response.
    async fn http_query(&self, sql: &str) -> Result<ChResponse, ConnectorError> {
        http_query(
            &self.client,
            &self.url,
            &self.user,
            &self.password,
            &self.database,
            sql,
        )
        .await
    }
}

// ── HTTP helper ────────────────────────────────────────────────────────────────

/// POST `sql` to the ClickHouse HTTP endpoint and parse the JSONCompact response.
async fn http_query(
    client: &reqwest::Client,
    url: &str,
    user: &str,
    password: &str,
    database: &str,
    sql: &str,
) -> Result<ChResponse, ConnectorError> {
    let body = format!("{sql} FORMAT JSONCompact");

    let response = client
        .post(url)
        .header("X-ClickHouse-User", user)
        .header("X-ClickHouse-Key", password)
        .header("X-ClickHouse-Database", database)
        .body(body.clone())
        .send()
        .await
        .map_err(|e| ConnectorError::QueryFailed {
            sql: sql.to_string(),
            message: e.to_string(),
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(ConnectorError::QueryFailed {
            sql: sql.to_string(),
            message: format!("HTTP {status}: {text}"),
        });
    }

    let text = response
        .text()
        .await
        .map_err(|e| ConnectorError::QueryFailed {
            sql: sql.to_string(),
            message: e.to_string(),
        })?;

    serde_json::from_str::<ChResponse>(&text).map_err(|e| ConnectorError::QueryFailed {
        sql: sql.to_string(),
        message: format!("JSON parse error: {e}\nResponse: {text}"),
    })
}

// ── DatabaseConnector impl ────────────────────────────────────────────────────

#[async_trait]
impl DatabaseConnector for ClickHouseConnector {
    fn dialect(&self) -> SqlDialect {
        SqlDialect::Other("ClickHouse")
    }

    async fn execute_query(
        &self,
        sql: &str,
        sample_limit: u64,
    ) -> Result<ExecutionResult, ConnectorError> {
        let sql = normalize_sql(sql);
        // 1. Total row count via subquery.
        let count_sql = format!("SELECT count() FROM ({sql})");
        let count_resp = self.http_query(&count_sql).await?;
        let total_row_count: u64 = count_resp
            .data
            .first()
            .and_then(|r| r.first())
            .and_then(|v| match v {
                Value::Number(n) => n.as_u64(),
                Value::String(s) => s.parse().ok(),
                _ => None,
            })
            .unwrap_or(0);

        // 2. Sample rows.
        let sample_sql = format!("SELECT * FROM ({sql}) LIMIT {sample_limit}");
        let sample_resp = self.http_query(&sample_sql).await?;

        let column_names: Vec<String> = sample_resp.meta.iter().map(|m| m.name.clone()).collect();
        let column_types: Vec<Option<String>> =
            sample_resp.meta.iter().map(|m| m.r#type.clone()).collect();
        let col_count = column_names.len();

        let sample_rows: Vec<QueryRow> = sample_resp
            .data
            .iter()
            .map(|row| {
                let cells = (0..col_count)
                    .map(|i| row.get(i).map(json_to_cell).unwrap_or(CellValue::Null))
                    .collect();
                QueryRow(cells)
            })
            .collect();

        // 3. Per-column stats.
        let mut col_stats: Vec<ColumnStats> = Vec::with_capacity(col_count);
        for (idx, col) in column_names.iter().enumerate() {
            let quoted = format!("\"{}\"", col.replace('"', "\\\""));
            let col_type = column_types
                .get(idx)
                .and_then(|t| t.as_deref())
                .unwrap_or("");
            // toFloat64OrNull only accepts String in ClickHouse. Route by type:
            // - Numeric: avg/stddevPop work directly on numeric columns and
            //   skip NULLs natively.
            // - String:  wrap in toFloat64OrNull so non-numeric strings become
            //   NULL; avg/stddevPop then skip them.
            // - Other:   dates, UUIDs, etc. — emit NULL for mean/stddev.
            let (avg_expr, sd_expr) = match clickhouse_type_category(col_type) {
                TypeCategory::Numeric => (format!("avg({quoted})"), format!("stddevPop({quoted})")),
                TypeCategory::String => (
                    format!("avg(toFloat64OrNull({quoted}))"),
                    format!("stddevPop(toFloat64OrNull({quoted}))"),
                ),
                TypeCategory::Other => (
                    "CAST(NULL AS Nullable(Float64))".to_string(),
                    "CAST(NULL AS Nullable(Float64))".to_string(),
                ),
            };
            let stat_sql = format!(
                "SELECT \
                    countIf(isNull({quoted})) AS nc, \
                    uniqExact({quoted}) AS dc, \
                    toString(min({quoted})) AS mn, \
                    toString(max({quoted})) AS mx, \
                    {avg_expr} AS avg_v, \
                    {sd_expr} AS sd_v \
                 FROM ({sql})"
            );

            let stat_resp = self.http_query(&stat_sql).await?;
            let stat_row = stat_resp.data.first();

            let null_count: u64 = stat_row
                .and_then(|r| r.first())
                .and_then(|v| match v {
                    Value::Number(n) => n.as_u64(),
                    Value::String(s) => s.parse().ok(),
                    _ => None,
                })
                .unwrap_or(0);
            let distinct_count: u64 = stat_row
                .and_then(|r| r.get(1))
                .and_then(|v| match v {
                    Value::Number(n) => n.as_u64(),
                    Value::String(s) => s.parse().ok(),
                    _ => None,
                })
                .unwrap_or(0);
            let min_v = stat_row
                .and_then(|r| r.get(2))
                .map(json_to_cell)
                .unwrap_or(CellValue::Null);
            let max_v = stat_row
                .and_then(|r| r.get(3))
                .map(json_to_cell)
                .unwrap_or(CellValue::Null);
            let mean = stat_row.and_then(|r| r.get(4)).and_then(|v| match v {
                Value::Number(n) => n.as_f64(),
                Value::String(s) => s.parse().ok(),
                _ => None,
            });
            let std_dev = stat_row.and_then(|r| r.get(5)).and_then(|v| match v {
                Value::Number(n) => n.as_f64(),
                Value::String(s) => s.parse().ok(),
                _ => None,
            });

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

    async fn execute_query_full(&self, sql: &str) -> Result<TypedRowStream, ConnectorError> {
        let sql = normalize_sql(sql);
        // One request: `SELECT * FROM (user_sql) FORMAT JSONCompact`.
        // The response carries per-column `meta.type` strings (Nullable,
        // LowCardinality, composites all included) and a row-major `data`
        // array of JSON values, which `parse_ch_cell` decodes typed.
        let full_sql = format!("SELECT * FROM ({sql})");
        let resp = self.http_query(&full_sql).await?;

        let columns: Vec<ColumnSpec> = resp
            .meta
            .iter()
            .map(|m| ColumnSpec {
                name: m.name.clone(),
                data_type: ch_type_to_typed(m.r#type.as_deref().unwrap_or("")),
            })
            .collect();
        let col_count = columns.len();

        let typed_rows: Vec<Result<Vec<TypedValue>, TypedRowError>> = resp
            .data
            .iter()
            .map(|row| {
                let mut cells = Vec::with_capacity(col_count);
                for (idx, col) in columns.iter().enumerate() {
                    let v = row.get(idx).unwrap_or(&Value::Null);
                    cells.push(parse_ch_cell(v, col)?);
                }
                Ok(cells)
            })
            .collect();

        Ok(TypedRowStream::from_rows(columns, typed_rows))
    }

    fn introspect_schema(&self) -> Result<SchemaInfo, ConnectorError> {
        Ok(self.cached_schema.clone())
    }
}

// ── Schema pre-fetch ──────────────────────────────────────────────────────────

/// Query `system.columns` and build a [`SchemaInfo`].
async fn fetch_schema(
    client: &reqwest::Client,
    url: &str,
    user: &str,
    password: &str,
    database: &str,
) -> Result<SchemaInfo, ConnectorError> {
    // Escape single quotes in the database name.
    let db_escaped = database.replace('\'', "\\'");
    let schema_sql = format!(
        "SELECT table, name, type \
         FROM system.columns \
         WHERE database = '{db_escaped}' \
         ORDER BY table, position"
    );

    let resp = http_query(client, url, user, password, database, &schema_sql).await?;

    let mut map: HashMap<String, Vec<SchemaColumnInfo>> = HashMap::new();
    for row in &resp.data {
        let table = match row.first() {
            Some(Value::String(s)) => s.clone(),
            _ => continue,
        };
        let column = match row.get(1) {
            Some(Value::String(s)) => s.clone(),
            _ => continue,
        };
        let data_type = match row.get(2) {
            Some(Value::String(s)) => s.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn is_numeric(t: &str) -> bool {
        matches!(clickhouse_type_category(t), TypeCategory::Numeric)
    }
    fn is_string(t: &str) -> bool {
        matches!(clickhouse_type_category(t), TypeCategory::String)
    }
    fn is_other(t: &str) -> bool {
        matches!(clickhouse_type_category(t), TypeCategory::Other)
    }

    #[test]
    fn type_category_numerics() {
        assert!(is_numeric("Float64"));
        assert!(is_numeric("Float32"));
        assert!(is_numeric("Int32"));
        assert!(is_numeric("Int64"));
        assert!(is_numeric("Int128"));
        assert!(is_numeric("UInt64"));
        assert!(is_numeric("UInt32"));
        assert!(is_numeric("Decimal(18, 4)"));
        assert!(is_numeric("Decimal128(38, 10)"));
    }

    #[test]
    fn type_category_nullable_numerics() {
        assert!(is_numeric("Nullable(Float64)"));
        assert!(is_numeric("Nullable(Int32)"));
        assert!(is_numeric("Nullable(UInt64)"));
        assert!(is_numeric("Nullable(Decimal(10, 2))"));
    }

    #[test]
    fn type_category_strings() {
        assert!(is_string("String"));
        assert!(is_string("FixedString"));
        // Realistic forms returned by ClickHouse: FixedString always carries a
        // length, and both kinds are commonly Nullable.
        assert!(is_string("FixedString(36)"));
        assert!(is_string("Nullable(FixedString(16))"));
        assert!(is_string("Nullable(String)"));
    }

    #[test]
    fn type_category_low_cardinality() {
        // LowCardinality is a dictionary-encoding wrapper; the inner type
        // determines the category. Common in production for low-cardinality
        // string columns (status, country, etc.) and occasionally numerics.
        assert!(is_string("LowCardinality(String)"));
        assert!(is_string("LowCardinality(FixedString(16))"));
        assert!(is_numeric("LowCardinality(Float64)"));
        assert!(is_numeric("LowCardinality(Int32)"));
        // Either nesting order is legal in ClickHouse.
        assert!(is_string("LowCardinality(Nullable(String))"));
        assert!(is_string("Nullable(LowCardinality(String))"));
        assert!(is_numeric("Nullable(LowCardinality(Float64))"));
    }

    #[test]
    fn type_category_other() {
        assert!(is_other("Date"));
        assert!(is_other("Date32"));
        assert!(is_other("DateTime"));
        assert!(is_other("DateTime64(3)"));
        assert!(is_other("UUID"));
        assert!(is_other("Bool"));
        assert!(is_other(""));
        assert!(is_other("Array(String)"));
    }
}
