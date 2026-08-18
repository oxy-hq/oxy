//! PostgreSQL connector implementation.
//!
//! Connection is **lazy**: `new()` only stores credentials; the first call to
//! `execute_query` or `execute_query_full` opens the TCP connection and
//! pre-fetches the schema.  This matches the old ConnectorX behaviour where
//! a miss-configured database only surfaced an error at query time, not at
//! connector-build time.
//!
//! `execute_query` (analytics FSM) still uses the temp-table approach for
//! bounded sampling + per-column statistics.  `execute_query_full` (IDE path)
//! uses `prepare` + an inline cast subquery — no temp tables, no round-trips
//! to `pg_attribute`.  Non-preparable statements (DDL, `SHOW`, `SET`) fall
//! back to the simple-query protocol, returning all values as text.

use std::collections::HashMap;

use async_trait::async_trait;
use tokio_postgres::{Client, NoTls};

use agentic_core::result::{
    CellValue, ColumnSpec, QueryResult, QueryRow, TypedDataType, TypedRowError, TypedRowStream,
    TypedValue,
};

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::connector::{
    ColumnStats, ConnectorError, DatabaseConnector, ExecutionResult, QueryFailedDetails, ResultCap,
    ResultSummary, SchemaColumnInfo, SchemaInfo, SchemaTableInfo, SqlDialect, estimate_row_bytes,
    is_returning_statement, normalize_sql, plan_sql_script,
};
use crate::postgres_typed::{decode_row, pg_typname_to_typed, select_expr_for_pg_type};

// ── Error helpers ─────────────────────────────────────────────────────────────

/// Flatten a `tokio_postgres` error into a single human-readable string.
///
/// Used at sites that don't carry the originating SQL (e.g. transport / TLS
/// failures, schema introspection). Query-execution sites should use
/// [`pg_query_failed`] so SQLSTATE / DETAIL / HINT / POSITION reach the
/// caller as separate fields.
fn pg_error_message(e: &tokio_postgres::Error) -> String {
    if let Some(db) = e.as_db_error() {
        let mut msg = format!("[{}] {}", db.code().code(), db.message());
        if let Some(detail) = db.detail() {
            msg.push_str(&format!(" — {detail}"));
        }
        if let Some(hint) = db.hint() {
            msg.push_str(&format!(" (hint: {hint})"));
        }
        msg
    } else {
        e.to_string()
    }
}

/// Build a structured [`QueryFailedDetails`] from a `tokio_postgres` error.
///
/// For server-side errors the SQLSTATE, message, DETAIL, HINT, and POSITION
/// each land in their own field so the IDE can render them as a structured
/// block (and highlight the offending token via `position`). Transport-level
/// errors only populate `message`.
fn pg_query_failed(sql: &str, e: &tokio_postgres::Error) -> QueryFailedDetails {
    let sql = sql.to_string();
    if let Some(db) = e.as_db_error() {
        QueryFailedDetails {
            sql,
            message: db.message().to_string(),
            code: Some(db.code().code().to_string()),
            detail: db.detail().map(|s| s.to_string()),
            hint: db.hint().map(|s| s.to_string()),
            position: db.position().and_then(|p| match p {
                tokio_postgres::error::ErrorPosition::Original(n) => Some(*n),
                tokio_postgres::error::ErrorPosition::Internal { .. } => None,
            }),
        }
    } else {
        QueryFailedDetails {
            sql,
            message: e.to_string(),
            ..Default::default()
        }
    }
}

// ── Value helpers ─────────────────────────────────────────────────────────────

fn pg_text_to_cell(opt: Option<String>) -> CellValue {
    match opt {
        None => CellValue::Null,
        Some(s) => {
            if let Ok(n) = s.parse::<i64>() {
                CellValue::Number(n as f64)
            } else if let Ok(n) = s.parse::<f64>() {
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
/// The client is created lazily on the first query so that a misconfigured
/// database only surfaces an error at query time, not at connector-build time.
/// The Mutex serialises concurrent queries; `tokio_postgres::Client` itself
/// is not `Sync`.
pub struct PostgresConnector {
    config: tokio_postgres::Config,
    /// `None` until the first query; `Some` after a successful connection.
    client: tokio::sync::Mutex<Option<Client>>,
    /// Populated after the first successful connection; empty until then.
    cached_schema: std::sync::RwLock<SchemaInfo>,
    /// Set when schema pre-fetch fails so `introspect_schema` can surface the error.
    schema_error: std::sync::RwLock<Option<String>>,
    /// In-pod memory backstop for `execute_query_full` (see [`ResultCap`]).
    result_cap: ResultCap,
}

impl PostgresConnector {
    /// Build a connector that will connect lazily on the first query.
    ///
    /// Never fails — connection errors surface when a query is first executed.
    /// Schema is populated on the first successful connection and cached for
    /// subsequent `introspect_schema` calls.
    pub fn new(host: &str, port: u16, user: &str, password: &str, database: &str) -> Self {
        let mut config = tokio_postgres::Config::new();
        config.host(host);
        config.port(port);
        config.user(user);
        config.password(password);
        config.dbname(database);
        Self {
            config,
            client: tokio::sync::Mutex::new(None),
            cached_schema: std::sync::RwLock::new(SchemaInfo::default()),
            schema_error: std::sync::RwLock::new(None),
            result_cap: ResultCap::default(),
        }
    }

    /// Override the [`ResultCap`] memory backstop. Primarily for tests that need
    /// to trip the guard on a small result.
    pub fn with_result_cap(mut self, cap: ResultCap) -> Self {
        self.result_cap = cap;
        self
    }
}

// ── DatabaseConnector impl ────────────────────────────────────────────────────

// ── Lazy-connection helper ────────────────────────────────────────────────────

/// Open a `tokio_postgres` connection if `opt_client` is still `None`.
///
/// On success `*opt_client` is set to `Some(client)` and `schema_lock` is
/// updated with the freshly-fetched schema.  Schema fetch failures are logged
/// and recorded in `schema_error` so `introspect_schema` can surface them,
/// but they do not prevent queries from running.
async fn ensure_client_connected(
    config: &tokio_postgres::Config,
    opt_client: &mut Option<Client>,
    schema_lock: &std::sync::RwLock<SchemaInfo>,
    schema_error: &std::sync::RwLock<Option<String>>,
) -> Result<(), ConnectorError> {
    if opt_client.is_some() {
        return Ok(());
    }
    let (client, connection) = config
        .connect(NoTls)
        .await
        .map_err(|e| ConnectorError::ConnectionError(pg_error_message(&e)))?;
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            tracing::error!("postgres connection driver error: {e}");
        }
    });
    match fetch_schema(&client).await {
        Ok(schema) => {
            if let Ok(mut g) = schema_lock.write() {
                *g = schema;
            }
        }
        Err(e) => {
            tracing::warn!("postgres: schema prefetch failed ({e}); schema browsing unavailable");
            if let Ok(mut g) = schema_error.write() {
                *g = Some(e.to_string());
            }
        }
    }
    *opt_client = Some(client);
    Ok(())
}

// ── DatabaseConnector impl ────────────────────────────────────────────────────

#[async_trait]
impl DatabaseConnector for PostgresConnector {
    fn dialect(&self) -> SqlDialect {
        SqlDialect::Postgres
    }

    /// Opens its **own** connection rather than borrowing `self.client` — see
    /// `postgres_tx`'s module docs for why sharing it would block every other
    /// reader of this database for the lifetime of an app's transaction.
    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::transaction::SqlTransaction>, ConnectorError> {
        let tx = crate::postgres_tx::PgTransaction::begin(&self.config).await?;
        Ok(Box::new(tx))
    }

    async fn execute_query(
        &self,
        sql: &str,
        sample_limit: u64,
    ) -> Result<ExecutionResult, ConnectorError> {
        // The temp-table wrap below (`CREATE TEMP TABLE _t AS (sql)`) is
        // only valid for one SELECT-family statement. A multi-statement
        // DDL/DML script (automation `execute_sql` setup files, Airhouse
        // bootstrap, etc.) substituted into the parens would fail with
        // `syntax error at or near "CREATE"`. Run leading statements for
        // their side effects, then sample only the final statement.
        // (Airhouse routes through this Postgres connector.)
        let script = plan_sql_script(sql);
        let mut guard = self.client.lock().await;
        ensure_client_connected(
            &self.config,
            &mut guard,
            &self.cached_schema,
            &self.schema_error,
        )
        .await?;
        let client = guard.as_ref().unwrap();

        for stmt in &script.prefix {
            client
                .batch_execute(stmt)
                .await
                .map_err(|e| ConnectorError::QueryFailed(pg_query_failed(stmt, &e)))?;
        }

        let sql = normalize_sql(&script.final_stmt);

        // Non-returning final statement (trailing DDL/DML): run it for the
        // side effect and return an empty result rather than wrapping
        // non-returning SQL in the sampling subquery.
        if !is_returning_statement(sql) {
            if !sql.is_empty() {
                client
                    .batch_execute(sql)
                    .await
                    .map_err(|e| ConnectorError::QueryFailed(pg_query_failed(sql, &e)))?;
            }
            return Ok(ExecutionResult::empty());
        }

        let tmp = "_agentic_tmp";

        // 1. Drop any leftover temp table from a previous (failed) execution.
        client
            .execute(&format!("DROP TABLE IF EXISTS {tmp}"), &[])
            .await
            .ok();

        // 2. Materialise the final query into a temp table.
        let create_sql = format!("CREATE TEMP TABLE {tmp} AS ({sql})");
        client
            .execute(&create_sql, &[])
            .await
            .map_err(|e| ConnectorError::QueryFailed(pg_query_failed(sql, &e)))?;

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

        let attr_rows = client.query(attr_sql, &[]).await.map_err(|e| {
            ConnectorError::query_failed(attr_sql.to_string(), pg_error_message(&e))
        })?;

        let column_names: Vec<String> = attr_rows.iter().map(|r| r.get::<_, String>(0)).collect();
        let column_types: Vec<String> = attr_rows.iter().map(|r| r.get::<_, String>(1)).collect();

        // 4. Total row count.
        let count_sql = format!("SELECT COUNT(*) FROM {tmp}");
        let count_row = client
            .query_one(&count_sql, &[])
            .await
            .map_err(|e| ConnectorError::query_failed(count_sql.clone(), pg_error_message(&e)))?;
        let total_row_count = count_row.get::<_, i64>(0) as u64;

        // 5. Sample rows — cast every column to TEXT so we can decode uniformly.
        let col_count = column_names.len();
        let sample_rows: Vec<QueryRow> = if col_count == 0 {
            Vec::new()
        } else {
            let cast_cols: String = column_names
                .iter()
                .map(|c| format!("\"{}\"::TEXT", c.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(", ");
            let sample_sql = format!("SELECT {cast_cols} FROM {tmp} LIMIT {sample_limit}");

            let rows = client.query(&sample_sql, &[]).await.map_err(|e| {
                ConnectorError::query_failed(sample_sql.clone(), pg_error_message(&e))
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
                ConnectorError::query_failed(basic_sql.clone(), pg_error_message(&e))
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

    async fn execute_query_full(&self, sql: &str) -> Result<TypedRowStream, ConnectorError> {
        let sql = normalize_sql(sql);
        let mut guard = self.client.lock().await;
        ensure_client_connected(
            &self.config,
            &mut guard,
            &self.cached_schema,
            &self.schema_error,
        )
        .await?;
        let client = guard.as_ref().unwrap();

        // Extended-protocol path: prepare the statement to get column type OIDs,
        // then execute a cast subquery that avoids temp-table creation/teardown.
        // Falls back to simple-query for DDL, DML, SHOW, and any statement the
        // server refuses to prepare.
        let stmt = match client.prepare(sql).await {
            Ok(s) => s,
            Err(_) => return execute_via_simple_query(client, sql).await,
        };

        // No RowDescription → DDL or DML without RETURNING. Execute the
        // statement and return an empty stream to signal success.
        if stmt.columns().is_empty() {
            client
                .execute(&stmt, &[])
                .await
                .map_err(|e| ConnectorError::QueryFailed(pg_query_failed(sql, &e)))?;
            return Ok(TypedRowStream::from_rows(vec![], vec![]));
        }

        // Derive column specs and per-column cast expressions from the
        // statement descriptor — no pg_attribute round-trip needed.
        let pg_typnames: Vec<String> = stmt
            .columns()
            .iter()
            .map(|c| c.type_().name().to_string())
            .collect();
        let columns: Vec<ColumnSpec> = stmt
            .columns()
            .iter()
            .zip(pg_typnames.iter())
            .map(|(c, typname)| ColumnSpec {
                name: c.name().to_string(),
                data_type: pg_typname_to_typed(typname),
            })
            .collect();
        let cast_exprs: Vec<String> = stmt
            .columns()
            .iter()
            .zip(pg_typnames.iter())
            .map(|(c, typname)| {
                let quoted = format!("\"{}\"", c.name().replace('"', "\"\""));
                select_expr_for_pg_type(&quoted, typname)
            })
            .collect();

        // Inline subquery: no temp table, casts applied to the live result set.
        let cast_sql = format!("SELECT {} FROM ({sql}) __q", cast_exprs.join(", "));

        // Stream rows via `query_raw` and stop at the `ResultCap` so an unbounded
        // scan (an uncapped `world_model_graph` query, or 10k very wide rows)
        // can't materialize gigabytes in the pod. Plain `query()` would buffer
        // the whole result before we could cap it. The row that crosses the
        // threshold is kept; everything after is dropped and the result flagged
        // truncated.
        use futures::{TryStreamExt, pin_mut};
        let no_params: [&(dyn tokio_postgres::types::ToSql + Sync); 0] = [];
        let row_stream = client
            .query_raw(cast_sql.as_str(), no_params)
            .await
            .map_err(|e| ConnectorError::QueryFailed(pg_query_failed(sql, &e)))?;
        pin_mut!(row_stream);

        let cap = self.result_cap;
        let mut typed_rows: Vec<Result<Vec<TypedValue>, TypedRowError>> = Vec::new();
        let mut bytes: u64 = 0;
        let mut truncated = false;
        while let Some(row) = row_stream
            .try_next()
            .await
            .map_err(|e| ConnectorError::QueryFailed(pg_query_failed(sql, &e)))?
        {
            let decoded = decode_row(&row, &columns);
            if let Ok(values) = &decoded {
                bytes += estimate_row_bytes(values);
            }
            typed_rows.push(decoded);
            if cap.exceeded(typed_rows.len() as u64, bytes) {
                truncated = true;
                break;
            }
        }

        let stream = TypedRowStream::from_rows(columns, typed_rows);
        Ok(if truncated {
            stream.with_truncation(Arc::new(AtomicBool::new(true)))
        } else {
            stream
        })
    }

    async fn prepare_schema(&self) -> Result<(), ConnectorError> {
        let mut guard = self.client.lock().await;
        ensure_client_connected(
            &self.config,
            &mut guard,
            &self.cached_schema,
            &self.schema_error,
        )
        .await
    }

    fn introspect_schema(&self) -> Result<SchemaInfo, ConnectorError> {
        if let Ok(err_guard) = self.schema_error.read()
            && let Some(ref err) = *err_guard
        {
            return Err(ConnectorError::ConnectionError(format!(
                "schema introspection failed: {err}"
            )));
        }
        Ok(self
            .cached_schema
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone())
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
        ConnectorError::ConnectionError(format!(
            "schema introspection failed: {}",
            pg_error_message(&e)
        ))
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

// ── Simple-query fallback ─────────────────────────────────────────────────────

/// Execute `sql` via the simple-query protocol, returning all column values as
/// [`TypedValue::Text`].
///
/// Used as a fallback from [`PostgresConnector::execute_query_full`] when the
/// server refuses to prepare the statement (DDL, `SHOW`, `SET`, etc.).
async fn execute_via_simple_query(
    client: &Client,
    sql: &str,
) -> Result<TypedRowStream, ConnectorError> {
    use tokio_postgres::SimpleQueryMessage;

    let messages = client
        .simple_query(sql)
        .await
        .map_err(|e| ConnectorError::QueryFailed(pg_query_failed(sql, &e)))?;

    let mut columns: Vec<ColumnSpec> = Vec::new();
    let mut rows: Vec<Result<Vec<TypedValue>, TypedRowError>> = Vec::new();

    for msg in messages {
        if let SimpleQueryMessage::Row(row) = msg {
            if columns.is_empty() {
                columns = row
                    .columns()
                    .iter()
                    .map(|c| ColumnSpec {
                        name: c.name().to_string(),
                        data_type: TypedDataType::Text,
                    })
                    .collect();
            }
            let n = columns.len();
            let values = (0..n)
                .map(|i| {
                    row.get(i)
                        .map_or(TypedValue::Null, |s| TypedValue::Text(s.to_string()))
                })
                .collect();
            rows.push(Ok(values));
        }
    }

    Ok(TypedRowStream::from_rows(columns, rows))
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
