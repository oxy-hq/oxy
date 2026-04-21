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

mod conversion;
mod schema;

use async_trait::async_trait;
use snowflake_api::{QueryResult as SnowflakeQueryResult, SnowflakeApi};

use agentic_core::result::{CellValue, QueryResult, QueryRow};

use crate::connector::{
    ColumnStats, ConnectorError, DatabaseConnector, ExecutionResult, ResultSummary, SchemaInfo,
    SqlDialect,
};

use conversion::{arrow_to_cell, json_value_to_cell};
use schema::{StatRow, build_multi_stat_sql, decode_stat_rows, extract_count, fetch_schema};

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
                        min: Some(
                            row.as_ref()
                                .map(|r| r.min.clone())
                                .unwrap_or(CellValue::Null),
                        ),
                        max: Some(
                            row.as_ref()
                                .map(|r| r.max.clone())
                                .unwrap_or(CellValue::Null),
                        ),
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
