//! BigQuery connector implementation.
//!
//! Uses `gcp_bigquery_client` to run queries via the BigQuery Jobs API.
//! Authentication is via a service account key file.
//!
//! # Query flow
//!
//! 1. Build a `QueryRequest` with `use_legacy_sql: false`.
//! 2. POST via `client.job().query(project_id, request)` — returns `QueryResponse`.
//! 3. Parse column names from `response.schema.fields`.
//! 4. Iterate rows with `ResultSet::new_from_query_response(response)`.
//! 5. For stats, run a second query per column with COUNT, MIN, MAX, AVG,
//!    STDDEV_POP.
//!
//! Schema is introspected from `INFORMATION_SCHEMA.COLUMNS` and cached at
//! construction time because `introspect_schema()` is synchronous.

#![cfg(feature = "bigquery")]

use std::collections::HashMap;

use async_trait::async_trait;
use gcp_bigquery_client::{
    model::{query_request::QueryRequest, table_field_schema::TableFieldSchema},
    Client,
};

use agentic_core::result::{CellValue, QueryResult, QueryRow};

use crate::connector::{
    ColumnStats, ConnectorError, DatabaseConnector, ExecutionResult, ResultSummary,
    SchemaColumnInfo, SchemaInfo, SchemaTableInfo, SqlDialect,
};

// ── Connector ─────────────────────────────────────────────────────────────────

/// BigQuery connector backed by the `gcp_bigquery_client` crate.
pub struct BigQueryConnector {
    client: Client,
    project_id: String,
    dataset: Option<String>,
    cached_schema: SchemaInfo,
}

impl BigQueryConnector {
    /// Create a new connector authenticated with a service-account JSON key file.
    pub async fn new(
        key_path: &str,
        project_id: String,
        dataset: Option<String>,
    ) -> Result<Self, ConnectorError> {
        let client = Client::from_service_account_key_file(key_path)
            .await
            .map_err(|e| ConnectorError::ConnectionError(e.to_string()))?;

        let cached_schema = fetch_schema(&client, &project_id, dataset.as_deref())
            .await
            .unwrap_or_default();

        Ok(Self {
            client,
            project_id,
            dataset,
            cached_schema,
        })
    }
}

// ── DatabaseConnector impl ────────────────────────────────────────────────────

#[async_trait]
impl DatabaseConnector for BigQueryConnector {
    fn dialect(&self) -> SqlDialect {
        SqlDialect::BigQuery
    }

    async fn execute_query(
        &self,
        sql: &str,
        sample_limit: u64,
    ) -> Result<ExecutionResult, ConnectorError> {
        // 1. Run the user query with a row limit.
        let request = QueryRequest {
            query: sql.to_string(),
            use_legacy_sql: false,
            max_results: Some(sample_limit as i32),
            timeout_ms: Some(180_000),
            ..Default::default()
        };

        let response = self
            .client
            .job()
            .query(&self.project_id, request)
            .await
            .map_err(|e| ConnectorError::QueryFailed {
                sql: sql.to_string(),
                message: e.to_string(),
            })?;

        // 2. Column names and types from the response schema.
        let fields_ref = response.schema.as_ref().and_then(|s| s.fields.as_ref());
        let column_names: Vec<String> = fields_ref
            .map(|fields| fields.iter().map(|f| f.name.clone()).collect())
            .unwrap_or_default();
        let column_types: Vec<Option<String>> = fields_ref
            .map(|fields| {
                fields
                    .iter()
                    .map(|f| {
                        serde_json::to_value(&f.r#type)
                            .ok()
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let col_count = column_names.len();

        // 3. Iterate rows via ResultSet.
        let mut rs = gcp_bigquery_client::model::query_response::ResultSet::new_from_query_response(
            response,
        );
        let mut sample_rows: Vec<QueryRow> = Vec::new();
        while rs.next_row() {
            let cells = column_names
                .iter()
                .map(|col| bq_value_to_cell(&rs, col))
                .collect();
            sample_rows.push(QueryRow(cells));
        }

        // 4. Total row count via a separate COUNT query.
        let count_sql = format!("SELECT COUNT(*) AS _cnt FROM ({sql})");
        let count_request = QueryRequest {
            query: count_sql.clone(),
            use_legacy_sql: false,
            timeout_ms: Some(180_000),
            ..Default::default()
        };
        let count_response = self
            .client
            .job()
            .query(&self.project_id, count_request)
            .await
            .map_err(|e| ConnectorError::QueryFailed {
                sql: count_sql.clone(),
                message: e.to_string(),
            })?;

        let mut count_rs =
            gcp_bigquery_client::model::query_response::ResultSet::new_from_query_response(
                count_response,
            );
        let total_row_count: u64 = if count_rs.next_row() {
            count_rs
                .get_i64_by_name("_cnt")
                .ok()
                .flatten()
                .map(|n| n as u64)
                .unwrap_or(sample_rows.len() as u64)
        } else {
            sample_rows.len() as u64
        };

        // 5. Per-column stats.
        let mut col_stats: Vec<ColumnStats> = Vec::with_capacity(col_count);
        for (idx, col) in column_names.iter().enumerate() {
            let backtick_col = format!("`{}`", col.replace('`', "\\`"));
            let stat_sql = format!(
                "SELECT \
                    COUNTIF({backtick_col} IS NULL) AS nc, \
                    COUNT(DISTINCT {backtick_col}) AS dc, \
                    CAST(MIN({backtick_col}) AS STRING) AS mn, \
                    CAST(MAX({backtick_col}) AS STRING) AS mx, \
                    AVG(SAFE_CAST({backtick_col} AS FLOAT64)) AS avg_v, \
                    STDDEV_POP(SAFE_CAST({backtick_col} AS FLOAT64)) AS sd_v \
                 FROM ({sql})"
            );
            let stat_request = QueryRequest {
                query: stat_sql.clone(),
                use_legacy_sql: false,
                timeout_ms: Some(180_000),
                ..Default::default()
            };

            match self
                .client
                .job()
                .query(&self.project_id, stat_request)
                .await
            {
                Ok(stat_resp) => {
                    let mut stat_rs = gcp_bigquery_client::model::query_response::ResultSet::new_from_query_response(stat_resp);
                    if stat_rs.next_row() {
                        let null_count = stat_rs
                            .get_i64_by_name("nc")
                            .ok()
                            .flatten()
                            .map(|n| n as u64)
                            .unwrap_or(0);
                        let distinct_count = stat_rs
                            .get_i64_by_name("dc")
                            .ok()
                            .flatten()
                            .map(|n| n as u64)
                            .unwrap_or(0);
                        let min_v = stat_rs
                            .get_string_by_name("mn")
                            .ok()
                            .flatten()
                            .map(|s| parse_bq_cell_str(&s))
                            .unwrap_or(CellValue::Null);
                        let max_v = stat_rs
                            .get_string_by_name("mx")
                            .ok()
                            .flatten()
                            .map(|s| parse_bq_cell_str(&s))
                            .unwrap_or(CellValue::Null);
                        let mean = stat_rs.get_f64_by_name("avg_v").ok().flatten();
                        let std_dev = stat_rs.get_f64_by_name("sd_v").ok().flatten();
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
                    } else {
                        col_stats.push(empty_col_stats(col));
                    }
                }
                Err(_) => {
                    col_stats.push(empty_col_stats(col));
                }
            }
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

// ── Value helpers ─────────────────────────────────────────────────────────────

/// Read a column value from the current BigQuery `ResultSet` row into a
/// [`CellValue`], trying integer, float, and string in order.
fn bq_value_to_cell(
    rs: &gcp_bigquery_client::model::query_response::ResultSet,
    col: &str,
) -> CellValue {
    // Try integer first.
    if let Ok(Some(n)) = rs.get_i64_by_name(col) {
        return CellValue::Number(n as f64);
    }
    // Try float.
    if let Ok(Some(f)) = rs.get_f64_by_name(col) {
        return CellValue::Number(f);
    }
    // Fall back to string.
    match rs.get_string_by_name(col) {
        Ok(Some(s)) => parse_bq_cell_str(&s),
        _ => CellValue::Null,
    }
}

/// Parse a BigQuery string cell into a [`CellValue`], attempting numeric
/// conversion first.
fn parse_bq_cell_str(s: &str) -> CellValue {
    if let Ok(n) = s.parse::<f64>() {
        CellValue::Number(n)
    } else {
        CellValue::Text(s.to_string())
    }
}

/// Build an all-empty [`ColumnStats`] for columns where stats queries fail.
fn empty_col_stats(col: &str) -> ColumnStats {
    ColumnStats {
        name: col.to_string(),
        data_type: None,
        null_count: 0,
        distinct_count: None,
        min: None,
        max: None,
        mean: None,
        std_dev: None,
    }
}

// ── Schema pre-fetch ──────────────────────────────────────────────────────────

/// Fetch the schema from BigQuery `INFORMATION_SCHEMA.COLUMNS`.
///
/// If no dataset is provided, returns an empty schema.
async fn fetch_schema(
    client: &Client,
    project_id: &str,
    dataset: Option<&str>,
) -> Result<SchemaInfo, ConnectorError> {
    let Some(ds) = dataset else {
        return Ok(SchemaInfo::default());
    };

    let schema_sql = format!(
        "SELECT table_name, column_name, data_type \
         FROM `{project_id}.{ds}.INFORMATION_SCHEMA.COLUMNS` \
         ORDER BY table_name, ordinal_position"
    );

    let request = QueryRequest {
        query: schema_sql.clone(),
        use_legacy_sql: false,
        timeout_ms: Some(60_000),
        ..Default::default()
    };

    let response = client.job().query(project_id, request).await.map_err(|e| {
        ConnectorError::ConnectionError(format!("schema introspection failed: {e}"))
    })?;

    let mut rs =
        gcp_bigquery_client::model::query_response::ResultSet::new_from_query_response(response);

    let mut map: HashMap<String, Vec<SchemaColumnInfo>> = HashMap::new();
    while rs.next_row() {
        let table = match rs.get_string_by_name("table_name").ok().flatten() {
            Some(t) => t,
            None => continue,
        };
        let column = match rs.get_string_by_name("column_name").ok().flatten() {
            Some(c) => c,
            None => continue,
        };
        let data_type = rs
            .get_string_by_name("data_type")
            .ok()
            .flatten()
            .unwrap_or_default();

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
