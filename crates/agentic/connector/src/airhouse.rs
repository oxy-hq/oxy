//! Airhouse connector implementation.
//!
//! Airhouse speaks the PostgreSQL wire protocol but executes SQL in the DuckDB
//! dialect under the hood. This connector connects via `tokio_postgres` but:
//!
//! 1. Reports `SqlDialect::DuckDb` so solvers generate DuckDB-flavoured SQL.
//! 2. Uses the **simple query protocol** (`simple_query`) for every server call
//!    instead of the extended/prepared-statement protocol that [`PostgresConnector`]
//!    uses. Airhouse's extended-protocol column metadata is not fully compatible
//!    with `tokio_postgres` (row-index access fails with "invalid column `0`"),
//!    so we stick to `simple_query` which returns every value as a text string
//!    keyed by column name — a much smaller protocol surface.
//! 3. Uses DuckDB-native introspection (`information_schema.columns` with
//!    DuckDB semantics, `::VARCHAR` casts, `DOUBLE` type, `STDDEV_POP`).
//!
//! # WARNING: No TLS
//!
//! This connector uses [`tokio_postgres::NoTls`] — all traffic (credentials
//! **and** query data) is sent in plaintext. This is intentional for the
//! current deployment model where Airhouse runs on a private network segment
//! that is unreachable from the public internet. **Do not expose this connector
//! over an untrusted network without adding TLS.** A `tls: bool` field in the
//! config is the tracked follow-up for operators who need transport encryption.

use std::collections::HashMap;
use std::sync::Arc;

use uuid::Uuid;

use async_trait::async_trait;
use tokio_postgres::{Client, NoTls, SimpleQueryMessage};

use agentic_core::result::{
    BoxedRowStream, CellValue, ColumnSpec, QueryResult, QueryRow, TypedRowError, TypedRowStream,
    TypedValue,
};

use crate::airhouse_typed::{describe_type_to_typed, parse_cell};
use crate::connector::{
    ColumnStats, ConnectorError, DatabaseConnector, ExecutionResult, ResultSummary,
    SchemaColumnInfo, SchemaInfo, SchemaTableInfo, SqlDialect, normalize_sql,
};

// ── Value helpers ─────────────────────────────────────────────────────────────

/// Parse a text cell from Airhouse (all simple-query values come back as text)
/// into a [`CellValue`].
fn text_to_cell(opt: Option<&str>) -> CellValue {
    match opt {
        None => CellValue::Null,
        Some(s) => {
            if let Ok(n) = s.parse::<i64>() {
                CellValue::Number(n as f64)
            } else if let Ok(n) = s.parse::<f64>() {
                CellValue::Number(n)
            } else {
                CellValue::Text(s.to_string())
            }
        }
    }
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

// ── Connector ─────────────────────────────────────────────────────────────────

/// Airhouse-backed connector: pgwire transport, DuckDB dialect.
pub struct AirhouseConnector {
    client: Arc<tokio::sync::Mutex<Client>>,
    cached_schema: SchemaInfo,
}

impl AirhouseConnector {
    /// Connect to an Airhouse instance and pre-fetch the database schema.
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

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                tracing::error!("airhouse connection driver error: {e}");
            }
        });

        let cached_schema = fetch_schema(&client).await?;

        Ok(Self {
            client: Arc::new(tokio::sync::Mutex::new(client)),
            cached_schema,
        })
    }
}

// ── DatabaseConnector impl ────────────────────────────────────────────────────

#[async_trait]
impl DatabaseConnector for AirhouseConnector {
    fn dialect(&self) -> SqlDialect {
        SqlDialect::DuckDb
    }

    async fn execute_query(
        &self,
        sql: &str,
        sample_limit: u64,
    ) -> Result<ExecutionResult, ConnectorError> {
        let sql = normalize_sql(sql);
        let client = self.client.lock().await;
        let tmp = "_agentic_tmp";

        // Drop any leftover temp table from a previous (failed) execution.
        let _ = client
            .simple_query(&format!("DROP TABLE IF EXISTS {tmp}"))
            .await;

        // 1. Materialise the user query into a temp table.
        let create_sql = format!("CREATE TEMP TABLE {tmp} AS ({sql})");
        client
            .simple_query(&create_sql)
            .await
            .map_err(|e| ConnectorError::QueryFailed {
                sql: sql.to_string(),
                message: e.to_string(),
            })?;

        // 2. Column names + types via DuckDB's DESCRIBE.
        let describe_sql = format!("DESCRIBE {tmp}");
        let describe_messages =
            client
                .simple_query(&describe_sql)
                .await
                .map_err(|e| ConnectorError::QueryFailed {
                    sql: describe_sql.clone(),
                    message: e.to_string(),
                })?;

        let mut column_names: Vec<String> = Vec::new();
        let mut column_types: Vec<String> = Vec::new();
        for msg in &describe_messages {
            if let SimpleQueryMessage::Row(row) = msg {
                let name = row
                    .get("column_name")
                    .ok_or_else(|| ConnectorError::QueryFailed {
                        sql: describe_sql.clone(),
                        message: "DESCRIBE row missing column_name".to_string(),
                    })?
                    .to_string();
                let ty = row.get("column_type").unwrap_or_default().to_string();
                column_names.push(name);
                column_types.push(ty);
            }
        }
        let col_count = column_names.len();

        // 3. Total row count.
        let count_sql = format!("SELECT COUNT(*) AS n FROM {tmp}");
        let count_messages =
            client
                .simple_query(&count_sql)
                .await
                .map_err(|e| ConnectorError::QueryFailed {
                    sql: count_sql.clone(),
                    message: e.to_string(),
                })?;
        let total_row_count = count_messages
            .iter()
            .find_map(|m| match m {
                SimpleQueryMessage::Row(r) => r.get("n").and_then(|s| s.parse::<u64>().ok()),
                _ => None,
            })
            .unwrap_or(0);

        // 4. Sample rows — cast every column to VARCHAR for uniform decoding.
        let sample_rows: Vec<QueryRow> = if col_count == 0 {
            Vec::new()
        } else {
            let cast_cols: String = column_names
                .iter()
                .map(|c| {
                    let q = quote_ident(c);
                    format!("{q}::VARCHAR AS {q}")
                })
                .collect::<Vec<_>>()
                .join(", ");
            let sample_sql = format!("SELECT {cast_cols} FROM {tmp} LIMIT {sample_limit}");

            let messages = client.simple_query(&sample_sql).await.map_err(|e| {
                ConnectorError::QueryFailed {
                    sql: sample_sql.clone(),
                    message: e.to_string(),
                }
            })?;

            let mut rows = Vec::new();
            for msg in &messages {
                if let SimpleQueryMessage::Row(r) = msg {
                    let cells = column_names
                        .iter()
                        .map(|c| text_to_cell(r.get(c.as_str())))
                        .collect();
                    rows.push(QueryRow(cells));
                }
            }
            rows
        };

        // 5. Per-column stats — single batched query instead of 2N round-trips.
        //
        // Each column contributes 6 aliased expressions.  TRY_CAST to DOUBLE
        // returns NULL for non-numeric columns so mean/std_dev come back as
        // NULL naturally; no per-column error handling needed.
        let col_stats: Vec<ColumnStats> = if col_count == 0 {
            Vec::new()
        } else {
            let exprs: String = column_names
                .iter()
                .enumerate()
                .map(|(i, col)| {
                    let q = quote_ident(col);
                    format!(
                        "(COUNT(*) - COUNT({q}))::VARCHAR AS c{i}_nc, \
                         COUNT(DISTINCT {q})::VARCHAR AS c{i}_dc, \
                         MIN({q})::VARCHAR AS c{i}_mn, \
                         MAX({q})::VARCHAR AS c{i}_mx, \
                         AVG(TRY_CAST({q} AS DOUBLE))::VARCHAR AS c{i}_avg, \
                         STDDEV_POP(TRY_CAST({q} AS DOUBLE))::VARCHAR AS c{i}_sd"
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let stats_sql = format!("SELECT {exprs} FROM {tmp}");
            let stats_messages =
                client
                    .simple_query(&stats_sql)
                    .await
                    .map_err(|e| ConnectorError::QueryFailed {
                        sql: stats_sql.clone(),
                        message: e.to_string(),
                    })?;
            let stats_row = stats_messages.iter().find_map(|m| match m {
                SimpleQueryMessage::Row(r) => Some(r),
                _ => None,
            });

            column_names
                .iter()
                .enumerate()
                .map(|(i, col)| {
                    let r = stats_row.as_ref();
                    // SimpleQueryRow::get requires &str, not &String.
                    let k_nc = format!("c{i}_nc");
                    let k_dc = format!("c{i}_dc");
                    let k_mn = format!("c{i}_mn");
                    let k_mx = format!("c{i}_mx");
                    let k_avg = format!("c{i}_avg");
                    let k_sd = format!("c{i}_sd");
                    let null_count = r
                        .and_then(|r| r.get(k_nc.as_str()).and_then(|s| s.parse().ok()))
                        .unwrap_or(0u64);
                    let distinct_count = r
                        .and_then(|r| r.get(k_dc.as_str()).and_then(|s| s.parse().ok()))
                        .unwrap_or(0u64);
                    let min_v = text_to_cell(r.and_then(|r| r.get(k_mn.as_str())));
                    let max_v = text_to_cell(r.and_then(|r| r.get(k_mx.as_str())));
                    let mean = r.and_then(|r| r.get(k_avg.as_str()).and_then(|s| s.parse().ok()));
                    let std_dev = r.and_then(|r| r.get(k_sd.as_str()).and_then(|s| s.parse().ok()));
                    ColumnStats {
                        name: col.clone(),
                        data_type: column_types.get(i).cloned(),
                        null_count,
                        distinct_count: Some(distinct_count),
                        min: Some(min_v),
                        max: Some(max_v),
                        mean,
                        std_dev,
                    }
                })
                .collect()
        };

        // 6. Cleanup.
        let _ = client
            .simple_query(&format!("DROP TABLE IF EXISTS {tmp}"))
            .await;

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
        let tmp = format!("_agentic_typed_{}", Uuid::new_v4().simple());
        const PAGE_SIZE: usize = 1_000;

        // 1. Create temp table + introspect column types (one locked block).
        let columns = {
            let client = self.client.lock().await;

            let _ = client
                .simple_query(&format!("DROP TABLE IF EXISTS {tmp}"))
                .await;

            let create_sql = format!("CREATE TEMP TABLE {tmp} AS ({sql})");
            client
                .simple_query(&create_sql)
                .await
                .map_err(|e| ConnectorError::QueryFailed {
                    sql: sql.to_string(),
                    message: e.to_string(),
                })?;

            let describe_sql = format!("DESCRIBE {tmp}");
            let describe_messages = client.simple_query(&describe_sql).await.map_err(|e| {
                ConnectorError::QueryFailed {
                    sql: describe_sql.clone(),
                    message: e.to_string(),
                }
            })?;

            let mut columns: Vec<ColumnSpec> = Vec::new();
            for msg in &describe_messages {
                if let SimpleQueryMessage::Row(row) = msg {
                    let name = row
                        .get("column_name")
                        .ok_or_else(|| ConnectorError::QueryFailed {
                            sql: describe_sql.clone(),
                            message: "DESCRIBE row missing column_name".to_string(),
                        })?
                        .to_string();
                    let ty_str = row.get("column_type").unwrap_or_default();
                    columns.push(ColumnSpec {
                        name,
                        data_type: describe_type_to_typed(ty_str),
                    });
                }
            }
            columns
            // lock released here
        };

        if columns.is_empty() {
            let client = self.client.lock().await;
            let _ = client
                .simple_query(&format!("DROP TABLE IF EXISTS {tmp}"))
                .await;
            return Ok(TypedRowStream::from_rows(columns, vec![]));
        }

        // 2. Build the cast SELECT once; the stream owns it.
        let cast_cols: String = columns
            .iter()
            .map(|c| {
                let q = quote_ident(&c.name);
                format!("{q}::VARCHAR AS {q}")
            })
            .collect::<Vec<_>>()
            .join(", ");

        // 3. LIMIT/OFFSET stream: re-acquires the lock per page. Avoids SQL
        //    cursor syntax which DuckDB's pgwire server doesn't reliably support
        //    via the simple-query protocol.
        let client_arc = Arc::clone(&self.client);
        let columns_for_stream = columns.clone();
        let stream: BoxedRowStream = Box::pin(async_stream::stream! {
            let columns = columns_for_stream;
            let mut offset = 0usize;

            loop {
                let client = client_arc.lock().await;
                let page_sql = format!(
                    "SELECT {cast_cols} FROM {tmp} LIMIT {PAGE_SIZE} OFFSET {offset}"
                );

                match client.simple_query(&page_sql).await {
                    Err(e) => {
                        yield Err(TypedRowError::DriverError(e.to_string()));
                        break;
                    }
                    Ok(messages) => {
                        let batch: Vec<_> = messages
                            .into_iter()
                            .filter_map(|msg| match msg {
                                SimpleQueryMessage::Row(row) => {
                                    Some(parse_airhouse_row(&row, &columns))
                                }
                                _ => None,
                            })
                            .collect();

                        let fetched = batch.len();
                        for row in batch {
                            yield row;
                        }

                        if fetched < PAGE_SIZE {
                            // Last page — drop lock and clean up.
                            drop(client);
                            let cleanup = client_arc.lock().await;
                            let _ = cleanup
                                .simple_query(&format!("DROP TABLE IF EXISTS {tmp}"))
                                .await;
                            break;
                        }

                        offset += fetched;
                        // Lock released here between pages.
                    }
                }
            }
        });

        Ok(TypedRowStream {
            columns,
            rows: stream,
        })
    }

    fn introspect_schema(&self) -> Result<SchemaInfo, ConnectorError> {
        Ok(self.cached_schema.clone())
    }
}

/// Decode one row of the Airhouse simple-query result into [`TypedValue`]s.
///
/// Each column's string is parsed according to its pre-computed
/// [`ColumnSpec::data_type`]; NULL arrives as `None` and maps to
/// [`TypedValue::Null`].
fn parse_airhouse_row(
    row: &tokio_postgres::SimpleQueryRow,
    columns: &[ColumnSpec],
) -> Result<Vec<TypedValue>, TypedRowError> {
    let mut cells = Vec::with_capacity(columns.len());
    for col in columns {
        let cell = match row.get(col.name.as_str()) {
            None => TypedValue::Null,
            Some(text) => parse_cell(text, col)?,
        };
        cells.push(cell);
    }
    Ok(cells)
}

// ── Schema pre-fetch ──────────────────────────────────────────────────────────

/// Query `information_schema.columns` via the simple query protocol and build a
/// [`SchemaInfo`]. Filters out internal DuckLake / pg_catalog tables and our
/// own `_agentic_%` temp tables.
async fn fetch_schema(client: &Client) -> Result<SchemaInfo, ConnectorError> {
    let schema_sql = "\
        SELECT table_name, column_name, data_type \
        FROM information_schema.columns \
        WHERE table_schema NOT IN ('information_schema', 'pg_catalog', 'pg_toast', 'ducklake') \
          AND table_name NOT LIKE 'ducklake_%' \
          AND table_name NOT LIKE '_agentic_%' \
          AND table_name NOT LIKE 'airhouse_%' \
        ORDER BY table_name, ordinal_position";

    let messages = client.simple_query(schema_sql).await.map_err(|e| {
        ConnectorError::ConnectionError(format!("airhouse schema introspection failed: {e}"))
    })?;

    let mut map: HashMap<String, Vec<SchemaColumnInfo>> = HashMap::new();
    for msg in &messages {
        let SimpleQueryMessage::Row(row) = msg else {
            continue;
        };
        let table = match row.get("table_name") {
            Some(s) => s.to_string(),
            None => continue,
        };
        let column = match row.get("column_name") {
            Some(s) => s.to_string(),
            None => continue,
        };
        let data_type = row.get("data_type").unwrap_or_default().to_string();
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
