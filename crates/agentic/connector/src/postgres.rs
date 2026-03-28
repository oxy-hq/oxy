//! PostgreSQL connector implementation.
//!
//! Uses the temp-table pattern via `tokio_postgres`:
//! 1. `CREATE TEMP TABLE _agentic_tmp AS ({sql})` — execute once.
//! 2. Query `pg_attribute` to discover column names and types.
//! 3. `SELECT COUNT(*) FROM _agentic_tmp` — total row count.
//! 4. `SELECT "col1"::TEXT, "col2"::TEXT, ... FROM _agentic_tmp LIMIT n` — bounded sample.
//! 5. Per-column: `COUNT(*)-COUNT(col), COUNT(DISTINCT col), MIN::TEXT, MAX::TEXT`
//!    and optionally `AVG(col::DOUBLE PRECISION), STDDEV_POP(col::DOUBLE PRECISION)`.
//! 6. `DROP TABLE IF EXISTS _agentic_tmp` — cleanup.
//!
//! Schema introspection queries `information_schema.columns`, filtering out
//! system schemas.  The result is cached at construction time because
//! `introspect_schema()` is synchronous.

use std::collections::HashMap;

use async_trait::async_trait;
use tokio_postgres::{Client, NoTls};

use agentic_core::result::{CellValue, QueryResult, QueryRow};

use crate::connector::{
    ColumnStats, ConnectorError, DatabaseConnector, ExecutionResult, ResultSummary,
    SchemaColumnInfo, SchemaInfo, SchemaTableInfo, SqlDialect,
};

// ── Value helpers ─────────────────────────────────────────────────────────────

/// Parse a string cell from a TEXT-casted Postgres column into a [`CellValue`].
///
/// Attempts numeric parsing first; falls back to [`CellValue::Text`].
fn pg_text_to_cell(opt: Option<String>) -> CellValue {
    match opt {
        None => CellValue::Null,
        Some(s) => {
            if let Ok(n) = s.parse::<f64>() {
                CellValue::Number(n)
            } else {
                CellValue::Text(s)
            }
        }
    }
}

// ── Connector ─────────────────────────────────────────────────────────────────

/// PostgreSQL-backed connector for the agentic analytics FSM.
///
/// The Postgres client is not `Sync` by itself, but `tokio_postgres::Client`
/// allows concurrent requests via internal message-passing.  We wrap it in
/// a `tokio::sync::Mutex` so the connector satisfies `Send + Sync`.
pub struct PostgresConnector {
    client: tokio::sync::Mutex<Client>,
    cached_schema: SchemaInfo,
}

impl PostgresConnector {
    /// Connect to a PostgreSQL instance and pre-fetch the database schema.
    ///
    /// The connection driver task is spawned on the current Tokio runtime.
    pub async fn new(
        host: &str,
        port: u16,
        user: &str,
        password: &str,
        database: &str,
    ) -> Result<Self, ConnectorError> {
        let mut config = tokio_postgres::Config::new();
        config.host(host);
        config.port(port);
        config.user(user);
        config.password(password);
        config.dbname(database);

        let (client, connection) = config
            .connect(NoTls)
            .await
            .map_err(|e| ConnectorError::ConnectionError(e.to_string()))?;

        // Drive the connection in the background.
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                tracing::error!("postgres connection driver error: {e}");
            }
        });

        let cached_schema = fetch_schema(&client).await?;

        Ok(Self {
            client: tokio::sync::Mutex::new(client),
            cached_schema,
        })
    }
}

// ── DatabaseConnector impl ────────────────────────────────────────────────────

#[async_trait]
impl DatabaseConnector for PostgresConnector {
    fn dialect(&self) -> SqlDialect {
        SqlDialect::Postgres
    }

    async fn execute_query(
        &self,
        sql: &str,
        sample_limit: u64,
    ) -> Result<ExecutionResult, ConnectorError> {
        let client = self.client.lock().await;

        let tmp = "_agentic_tmp";

        // 1. Drop any leftover temp table from a previous (failed) execution.
        client
            .execute(&format!("DROP TABLE IF EXISTS {tmp}"), &[])
            .await
            .ok();

        // 2. Materialise the user query into a temp table.
        let create_sql = format!("CREATE TEMP TABLE {tmp} AS ({sql})");
        client
            .execute(&create_sql, &[])
            .await
            .map_err(|e| ConnectorError::QueryFailed {
                sql: sql.to_string(),
                message: e.to_string(),
            })?;

        // 3. Column names and types via pg_attribute.
        let attr_sql = "\
            SELECT a.attname, t.typname \
            FROM pg_attribute a \
            JOIN pg_class c ON c.oid = a.attrelid \
            JOIN pg_type  t ON t.oid = a.atttypid \
            WHERE c.relname = '_agentic_tmp' \
              AND a.attnum > 0 \
              AND NOT a.attisdropped \
            ORDER BY a.attnum";

        let attr_rows =
            client
                .query(attr_sql, &[])
                .await
                .map_err(|e| ConnectorError::QueryFailed {
                    sql: attr_sql.to_string(),
                    message: e.to_string(),
                })?;

        let column_names: Vec<String> = attr_rows.iter().map(|r| r.get::<_, String>(0)).collect();
        let column_types: Vec<String> = attr_rows.iter().map(|r| r.get::<_, String>(1)).collect();

        // 4. Total row count.
        let count_sql = format!("SELECT COUNT(*) FROM {tmp}");
        let count_row =
            client
                .query_one(&count_sql, &[])
                .await
                .map_err(|e| ConnectorError::QueryFailed {
                    sql: count_sql.clone(),
                    message: e.to_string(),
                })?;
        let total_row_count = count_row.get::<_, i64>(0) as u64;

        // 5. Sample rows — cast every column to TEXT so we can decode uniformly.
        let col_count = column_names.len();
        let sample_rows: Vec<QueryRow> =
            if col_count == 0 {
                Vec::new()
            } else {
                let cast_cols: String = column_names
                    .iter()
                    .map(|c| format!("\"{}\"::TEXT", c.replace('"', "\"\"")))
                    .collect::<Vec<_>>()
                    .join(", ");
                let sample_sql = format!("SELECT {cast_cols} FROM {tmp} LIMIT {sample_limit}");

                let rows = client.query(&sample_sql, &[]).await.map_err(|e| {
                    ConnectorError::QueryFailed {
                        sql: sample_sql.clone(),
                        message: e.to_string(),
                    }
                })?;

                rows.iter()
                    .map(|r| {
                        let cells = (0..col_count)
                            .map(|i| pg_text_to_cell(r.get::<_, Option<String>>(i)))
                            .collect();
                        QueryRow(cells)
                    })
                    .collect()
            };

        // 6. Per-column stats.
        let mut col_stats: Vec<ColumnStats> = Vec::with_capacity(col_count);
        for (idx, col) in column_names.iter().enumerate() {
            let quoted = format!("\"{}\"", col.replace('"', "\"\""));

            // Basic stats: null_count, distinct_count, min, max.
            let basic_sql = format!(
                "SELECT \
                    COUNT(*) - COUNT({quoted}), \
                    COUNT(DISTINCT {quoted}), \
                    MIN({quoted})::TEXT, \
                    MAX({quoted})::TEXT \
                 FROM {tmp}"
            );
            let basic_row = client.query_one(&basic_sql, &[]).await.map_err(|e| {
                ConnectorError::QueryFailed {
                    sql: basic_sql.clone(),
                    message: e.to_string(),
                }
            })?;

            let null_count = basic_row.get::<_, i64>(0) as u64;
            let distinct_count = basic_row.get::<_, i64>(1) as u64;
            let min_v = pg_text_to_cell(basic_row.get::<_, Option<String>>(2));
            let max_v = pg_text_to_cell(basic_row.get::<_, Option<String>>(3));

            // Numeric stats: AVG + STDDEV_POP — may fail for non-numeric columns.
            let numeric_sql = format!(
                "SELECT \
                    AVG({quoted}::DOUBLE PRECISION), \
                    STDDEV_POP({quoted}::DOUBLE PRECISION) \
                 FROM {tmp}"
            );
            let (mean, std_dev) = match client.query_one(&numeric_sql, &[]).await {
                Ok(row) => (row.get::<_, Option<f64>>(0), row.get::<_, Option<f64>>(1)),
                Err(_) => (None, None),
            };

            col_stats.push(ColumnStats {
                name: col.clone(),
                data_type: column_types.get(idx).cloned(),
                null_count,
                distinct_count: Some(distinct_count),
                min: Some(min_v),
                max: Some(max_v),
                mean,
                std_dev,
            });
        }

        // 7. Cleanup.
        client
            .execute(&format!("DROP TABLE IF EXISTS {tmp}"), &[])
            .await
            .ok();

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

// ── Schema pre-fetch ──────────────────────────────────────────────────────────

/// Query `information_schema.columns` and build a [`SchemaInfo`].
async fn fetch_schema(client: &Client) -> Result<SchemaInfo, ConnectorError> {
    let schema_sql = "\
        SELECT table_name, column_name, data_type \
        FROM information_schema.columns \
        WHERE table_schema NOT IN ('information_schema', 'pg_catalog', 'pg_toast') \
          AND table_name NOT LIKE '_agentic_%' \
        ORDER BY table_name, ordinal_position";

    let rows = client.query(schema_sql, &[]).await.map_err(|e| {
        ConnectorError::ConnectionError(format!("schema introspection failed: {e}"))
    })?;

    let mut map: HashMap<String, Vec<SchemaColumnInfo>> = HashMap::new();
    for row in &rows {
        let table: String = row.get(0);
        let column: String = row.get(1);
        let data_type: String = row.get(2);
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
