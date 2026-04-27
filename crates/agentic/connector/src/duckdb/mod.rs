//! DuckDB connector implementation.
//!
//! Uses the temp-table pattern:
//! 1. `CREATE OR REPLACE TEMP TABLE _agentic_tmp AS ({sql})` — execute once.
//! 2. `SELECT COUNT(*) FROM _agentic_tmp` — total row count.
//! 3. `SELECT * FROM _agentic_tmp LIMIT {sample_limit}` — bounded sample.
//! 4. Per-column: `COUNT()-COUNT(col), COUNT(DISTINCT col), MIN, MAX,
//!    AVG(TRY_CAST(col AS DOUBLE)), STDDEV_POP(TRY_CAST(col AS DOUBLE))`.
//! 5. `DROP TABLE IF EXISTS _agentic_tmp` — cleanup.
//!
//! File loading registers Parquet/CSV files as temp views or materialized temp
//! tables via `from_directory()` / `from_files()`.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

use async_trait::async_trait;
use duckdb::{Connection, types::Value};
use slugify::slugify;

use agentic_core::result::{CellValue, QueryResult, QueryRow};

use agentic_core::result::{ColumnSpec, TypedRowError, TypedRowStream, TypedValue};

use crate::connector::{
    ColumnStats, ConnectorError, DatabaseConnector, ExecutionResult, ResultSummary,
    SchemaColumnInfo, SchemaInfo, SchemaTableInfo, normalize_sql,
};

// Re-export Connection so callers / integration tests can construct connections
// without adding `duckdb` as a separate direct dependency.
pub use duckdb::Connection as DuckDbConnection;

// ── Load strategy & metadata ──────────────────────────────────────────────────

/// Controls whether a file is loaded lazily (view) or eagerly (temp table).
#[derive(Debug, Clone, Copy, Default)]
pub enum LoadStrategy {
    /// `CREATE TEMP VIEW` — zero memory, re-reads the file on each query.
    ///
    /// Good for large files or one-shot queries.
    #[default]
    View,
    /// `CREATE TEMP TABLE AS SELECT *` — materialized in DuckDB's in-process
    /// memory.  Good for small files or repeated queries.
    Materialized,
}

/// Metadata about a table / view registered with the connector.
#[derive(Debug, Clone)]
pub struct TableInfo {
    pub name: String,
    /// `(column_name, data_type)` pairs returned by `DESCRIBE`.
    pub columns: Vec<(String, String)>,
    pub source: TableSource,
}

/// Where a [`TableInfo`] came from.
#[derive(Debug, Clone)]
pub enum TableSource {
    File {
        path: PathBuf,
        strategy: LoadStrategy,
    },
    /// Already existed in the DuckDB connection before we touched it.
    Native,
}

mod conversion;
mod schema;

use conversion::{
    describe_type_to_typed, duckdb_to_cell, duckdb_to_cell_opt, duckdb_value_to_typed,
};
use schema::{describe_query, describe_table, detect_join_keys, parse_summarize_cell};

pub struct DuckDbConnector {
    conn: Mutex<Connection>,
    /// Tables / views registered during construction.
    loaded_tables: Vec<TableInfo>,
}

// ── Constructors ──────────────────────────────────────────────────────────────

impl DuckDbConnector {
    /// Wrap an existing, already-configured DuckDB connection.
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
            loaded_tables: Vec::new(),
        }
    }

    /// Fresh in-memory DuckDB instance with no pre-loaded tables.
    pub fn in_memory() -> Result<Self, ConnectorError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| ConnectorError::ConnectionError(e.to_string()))?;
        Ok(Self::new(conn))
    }

    /// Scan `dir` for `*.csv` and `*.parquet` files and register each as a
    /// view or temp table according to `strategy`.
    ///
    /// When two files share the same stem (e.g. `orders.csv` and
    /// `orders.parquet`) only the Parquet file is registered.
    pub fn from_directory(dir: &Path, strategy: LoadStrategy) -> Result<Self, ConnectorError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| ConnectorError::ConnectionError(e.to_string()))?;

        // Set file_search_path so queries referencing CSV/Parquet filenames
        // (e.g. `FROM 'cardio_4_4.csv'`) resolve to this directory.
        if let Ok(abs_dir) = dir.canonicalize() {
            let search_path_sql = format!("SET file_search_path = '{}'", abs_dir.display());
            let _ = conn.execute_batch(&search_path_sql);
        }

        // Collect candidates: stem → (abs_path, is_parquet).
        // Prefer Parquet over CSV on collision.
        let mut candidates: HashMap<String, (PathBuf, bool)> = HashMap::new();
        let entries = std::fs::read_dir(dir)
            .map_err(|e| ConnectorError::ConnectionError(format!("cannot read directory: {e}")))?;

        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();
            if ext != "csv" && ext != "parquet" {
                continue;
            }
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) if !s.is_empty() => s.to_string(),
                _ => continue,
            };
            let is_parquet = ext == "parquet";
            candidates
                .entry(stem)
                .and_modify(|e| {
                    if is_parquet {
                        *e = (path.clone(), true);
                    }
                })
                .or_insert((path, is_parquet));
        }

        let pairs: Vec<(PathBuf, LoadStrategy)> = candidates
            .into_values()
            .map(|(p, _)| (p, strategy))
            .collect();
        let file_refs: Vec<(&Path, LoadStrategy)> =
            pairs.iter().map(|(p, s)| (p.as_path(), *s)).collect();

        Self::from_files_with_conn(conn, &file_refs)
    }

    /// Register an explicit list of files, each with its own load strategy.
    ///
    /// # Example
    /// ```ignore
    /// DuckDbConnector::from_files(&[
    ///     (Path::new("small.csv"),     LoadStrategy::Materialized),
    ///     (Path::new("large.parquet"), LoadStrategy::View),
    /// ])
    /// ```
    pub fn from_files(files: &[(&Path, LoadStrategy)]) -> Result<Self, ConnectorError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| ConnectorError::ConnectionError(e.to_string()))?;
        Self::from_files_with_conn(conn, files)
    }

    // Shared registration logic used by both `from_directory` and `from_files`.
    fn from_files_with_conn(
        conn: Connection,
        files: &[(&Path, LoadStrategy)],
    ) -> Result<Self, ConnectorError> {
        let mut loaded_tables: Vec<TableInfo> = Vec::with_capacity(files.len());

        for (path, strategy) in files {
            let abs = path.canonicalize().map_err(|e| {
                ConnectorError::ConnectionError(format!("cannot resolve {}: {e}", path.display()))
            })?;
            let raw_stem = abs
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unnamed");
            let name = normalize_table_name(raw_stem);
            let full_name = abs
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string().replace('"', "\"\""));
            let ext = abs
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();

            let path_str = abs.display().to_string().replace('\'', "''");
            let src_expr = match ext.as_str() {
                "parquet" => format!("read_parquet('{path_str}')"),
                "csv" => format!("read_csv_auto('{path_str}')"),
                _ => format!("'{path_str}'"),
            };

            let create_sql = match strategy {
                LoadStrategy::View => {
                    format!(r#"CREATE OR REPLACE TEMP VIEW "{name}" AS SELECT * FROM {src_expr}"#)
                }
                LoadStrategy::Materialized => {
                    format!(r#"CREATE OR REPLACE TEMP TABLE "{name}" AS SELECT * FROM {src_expr}"#)
                }
            };

            conn.execute_batch(&create_sql)
                .map_err(|e| ConnectorError::QueryFailed {
                    sql: create_sql.clone(),
                    message: e.to_string(),
                })?;

            // Also expose the file under its full name (e.g. `oxymart.csv`) so
            // semantic-layer views that declare `table: "oxymart.csv"` resolve
            // without falling through to DuckDB's file-replacement scan, which
            // does not honor `file_search_path` for quoted identifiers.
            if let Some(full) = full_name.as_deref()
                && full != name
            {
                let alias_sql =
                    format!(r#"CREATE OR REPLACE TEMP VIEW "{full}" AS SELECT * FROM "{name}""#);
                conn.execute_batch(&alias_sql)
                    .map_err(|e| ConnectorError::QueryFailed {
                        sql: alias_sql.clone(),
                        message: e.to_string(),
                    })?;
            }

            let columns =
                describe_table(&conn, &name).map_err(|e| ConnectorError::QueryFailed {
                    sql: format!("DESCRIBE \"{name}\""),
                    message: e.to_string(),
                })?;

            loaded_tables.push(TableInfo {
                name,
                columns,
                source: TableSource::File {
                    path: abs,
                    strategy: *strategy,
                },
            });
        }

        Ok(Self {
            conn: Mutex::new(conn),
            loaded_tables,
        })
    }

    /// Tables / views registered during construction.
    pub fn loaded_tables(&self) -> &[TableInfo] {
        &self.loaded_tables
    }
}

// ── Table naming ─────────────────────────────────────────────────────────────

/// Derive a DuckDB table name from a file stem that is safe to reference
/// unquoted. Spaces, hyphens, and other non-identifier characters are
/// collapsed to `_`, and a leading digit is prefixed with `_` so the name is
/// a valid bare identifier.
///
/// Downstream consumers (including LLM-generated `.view.yml` files) often
/// round-trip identifiers through normalization, so keeping table names to
/// `[A-Za-z_][A-Za-z0-9_]*` avoids silent rename drift.
fn normalize_table_name(stem: &str) -> String {
    let slug = slugify!(stem, separator = "_");
    if slug.is_empty() {
        return "unnamed".to_string();
    }
    if slug.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("_{slug}")
    } else {
        slug
    }
}

// ── DatabaseConnector impl ────────────────────────────────────────────────────

#[async_trait]
impl DatabaseConnector for DuckDbConnector {
    fn dialect(&self) -> crate::connector::SqlDialect {
        crate::connector::SqlDialect::DuckDb
    }

    #[cfg(feature = "arrow")]
    fn as_arrow(&self) -> Option<&dyn crate::connector::AsArrowConnector> {
        Some(self)
    }

    async fn execute_query(
        &self,
        sql: &str,
        sample_limit: u64,
    ) -> Result<ExecutionResult, ConnectorError> {
        let sql = normalize_sql(sql);
        let conn = self
            .conn
            .lock()
            .map_err(|e| ConnectorError::ConnectionError(format!("mutex poisoned: {e}")))?;

        let tmp = "_agentic_tmp";

        // 1. Create the temp table once from the user's query.
        conn.execute_batch(&format!("DROP TABLE IF EXISTS {tmp};"))
            .map_err(|e| ConnectorError::QueryFailed {
                sql: sql.to_string(),
                message: e.to_string(),
            })?;

        conn.execute_batch(&format!("CREATE OR REPLACE TEMP TABLE {tmp} AS ({sql});"))
            .map_err(|e| ConnectorError::QueryFailed {
                sql: sql.to_string(),
                message: e.to_string(),
            })?;

        // 2. Total row count.
        let total_row_count: u64 = {
            let count_sql = format!("SELECT COUNT(*) FROM {tmp}");
            conn.query_row(&count_sql, [], |row| row.get::<_, i64>(0))
                .map_err(|e| ConnectorError::QueryFailed {
                    sql: count_sql,
                    message: e.to_string(),
                })? as u64
        };

        // 3a. Column names — use DESCRIBE on the temp table because duckdb-rs
        //     requires the statement to be executed before column_count()
        //     and column_names() are available, and we need them first.
        let described = describe_table(&conn, tmp).map_err(|e| ConnectorError::QueryFailed {
            sql: format!("DESCRIBE {tmp}"),
            message: e.to_string(),
        })?;
        let column_names: Vec<String> = described.iter().map(|(name, _)| name.clone()).collect();
        let column_types: Vec<String> = described.iter().map(|(_, ty)| ty.clone()).collect();

        // 3b. Sample rows.
        let col_count = column_names.len();
        let sample_rows: Vec<QueryRow> = {
            let sample_sql = format!("SELECT * FROM {tmp} LIMIT {sample_limit}");
            let mut stmt = conn
                .prepare(&sample_sql)
                .map_err(|e| ConnectorError::QueryFailed {
                    sql: sample_sql.clone(),
                    message: e.to_string(),
                })?;

            stmt.query_map([], |row| {
                let cells = (0..col_count)
                    .map(|i| row.get::<_, Value>(i).map(duckdb_to_cell))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(QueryRow(cells))
            })
            .map_err(|e| ConnectorError::QueryFailed {
                sql: sample_sql.clone(),
                message: e.to_string(),
            })?
            .collect::<Result<Vec<_>, duckdb::Error>>()
            .map_err(|e| ConnectorError::QueryFailed {
                sql: sample_sql,
                message: e.to_string(),
            })?
        };

        // 4. Per-column stats. Each column's query is best-effort: a
        // future DuckDB type, extension, or user-defined type without
        // MIN/MAX/COUNT(DISTINCT)/TRY_CAST AS DOUBLE support could
        // bind-error and otherwise tank the whole execute_query, surfacing
        // to the analytics agent as a generic "query failed" loop in the
        // reasoning trace. Match on the result and degrade just that
        // column's stats to None — same shape as the BigQuery connector.
        //
        // NB: the DuckDB version pinned today aggregates all built-in
        // complex types (MAP, LIST, STRUCT, UNION, BLOB, BIT) gracefully,
        // so this branch is defense-in-depth rather than a fix for an
        // observed bug.
        let mut col_stats: Vec<ColumnStats> = Vec::with_capacity(column_names.len());
        for (idx, col) in column_names.iter().enumerate() {
            let quoted = format!("\"{}\"", col.replace('"', "\"\""));
            let data_type = column_types.get(idx).cloned();
            let stat_sql = format!(
                "SELECT \
                    COUNT(*) - COUNT({quoted}), \
                    COUNT(DISTINCT {quoted}), \
                    MIN({quoted}), \
                    MAX({quoted}), \
                    AVG(TRY_CAST({quoted} AS DOUBLE)), \
                    STDDEV_POP(TRY_CAST({quoted} AS DOUBLE)) \
                 FROM {tmp}"
            );

            match conn.query_row(&stat_sql, [], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Value>(2)?,
                    row.get::<_, Value>(3)?,
                    row.get::<_, Option<f64>>(4)?,
                    row.get::<_, Option<f64>>(5)?,
                ))
            }) {
                Ok((null_count, distinct_count, min_v, max_v, mean, std_dev)) => {
                    col_stats.push(ColumnStats {
                        name: col.clone(),
                        data_type,
                        null_count: null_count as u64,
                        distinct_count: Some(distinct_count as u64),
                        min: Some(duckdb_to_cell(min_v)),
                        max: Some(duckdb_to_cell(max_v)),
                        mean,
                        std_dev,
                    });
                }
                Err(e) => {
                    // TODO(test): the bundled DuckDB version aggregates every
                    // built-in complex type successfully, so this arm has no
                    // realistic reproducer in `tests/duckdb_tests.rs`. If a
                    // future change here breaks the degraded-stats shape it
                    // won't be caught — revisit once an extension or UDT
                    // exists that genuinely bind-errors.
                    tracing::debug!(
                        column = %col,
                        column_type = ?data_type,
                        error = %e,
                        "DuckDB per-column stats query failed; degrading to empty stats \
                         (null_count is unknown, reported as 0)"
                    );
                    col_stats.push(ColumnStats {
                        name: col.clone(),
                        data_type,
                        null_count: 0,
                        distinct_count: None,
                        min: None,
                        max: None,
                        mean: None,
                        std_dev: None,
                    });
                }
            }
        }

        // 5. Clean up.
        let _ = conn.execute_batch(&format!("DROP TABLE IF EXISTS {tmp};"));

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
        let conn = self
            .conn
            .lock()
            .map_err(|e| ConnectorError::ConnectionError(format!("mutex poisoned: {e}")))?;

        // DESCRIBE resolves column names + types at the logical plan level —
        // no rows are fetched, no temp table needed.
        let described = describe_query(&conn, sql).map_err(|e| ConnectorError::QueryFailed {
            sql: format!("DESCRIBE ({sql})"),
            message: e.to_string(),
        })?;
        let columns: Vec<ColumnSpec> = described
            .iter()
            .map(|(name, ty)| ColumnSpec {
                name: name.clone(),
                data_type: describe_type_to_typed(ty),
            })
            .collect();
        let col_count = columns.len();
        let column_types: Vec<_> = columns.iter().map(|c| c.data_type.clone()).collect();

        let mut stmt = conn.prepare(sql).map_err(|e| ConnectorError::QueryFailed {
            sql: sql.to_string(),
            message: e.to_string(),
        })?;

        let rows_iter = stmt
            .query_map([], |row| {
                let mut cells = Vec::with_capacity(col_count);
                for i in 0..col_count {
                    let v: Value = row.get(i)?;
                    cells.push(duckdb_value_to_typed(v, &column_types[i]));
                }
                Ok(cells)
            })
            .map_err(|e| ConnectorError::QueryFailed {
                sql: sql.to_string(),
                message: e.to_string(),
            })?;

        // Collect eagerly so we can release the Mutex and return a `'static` stream.
        let mut rows: Vec<Result<Vec<TypedValue>, TypedRowError>> = Vec::new();
        for row in rows_iter {
            match row {
                Ok(cells) => rows.push(Ok(cells)),
                Err(e) => rows.push(Err(TypedRowError::DriverError(e.to_string()))),
            }
        }

        Ok(TypedRowStream::from_rows(columns, rows))
    }

    fn introspect_schema(&self) -> Result<SchemaInfo, ConnectorError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ConnectorError::ConnectionError(format!("mutex poisoned: {e}")))?;

        // ── 1. All non-system tables + views ──────────────────────────────────
        let table_rows: Vec<(String, String)> = conn
            .prepare(
                "SELECT table_schema, table_name \
                 FROM information_schema.tables \
                 WHERE table_schema NOT IN ('information_schema', 'pg_catalog') \
                   AND table_name NOT LIKE '_agentic_%' \
                 ORDER BY table_schema, table_name",
            )
            .map_err(|e| ConnectorError::Other(e.to_string()))?
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| ConnectorError::Other(e.to_string()))?
            .collect::<Result<_, duckdb::Error>>()
            .map_err(|e| ConnectorError::Other(e.to_string()))?;

        let mut tables: Vec<SchemaTableInfo> = Vec::with_capacity(table_rows.len());

        for (schema, table) in &table_rows {
            let qualified = format!("\"{schema}\".\"{table}\"");

            // ── 2. SUMMARIZE: one pass gives column_name, column_type, min, max.
            let summarize_rows: Vec<(String, String, Option<String>, Option<String>)> = conn
                .prepare(&format!("SUMMARIZE {qualified}"))
                .map_err(|e| ConnectorError::Other(e.to_string()))?
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                })
                .map_err(|e| ConnectorError::Other(e.to_string()))?
                .collect::<Result<_, duckdb::Error>>()
                .map_err(|e| ConnectorError::Other(e.to_string()))?;

            let col_count = summarize_rows.len();

            // ── 3. One sample query for all columns.
            let mut samples_by_idx: Vec<Vec<CellValue>> = vec![vec![]; col_count];
            if col_count > 0 {
                let sample_res = conn
                    .prepare(&format!("SELECT * FROM {qualified} LIMIT 5"))
                    .and_then(|mut stmt| {
                        stmt.query_map([], |row| {
                            (0..col_count)
                                .map(|i| row.get::<_, Value>(i))
                                .collect::<Result<Vec<_>, _>>()
                        })
                        .map(|mapped| mapped.collect::<Result<Vec<_>, _>>())
                    });
                if let Ok(Ok(rows)) = sample_res {
                    for row_vals in rows {
                        for (i, v) in row_vals.into_iter().enumerate() {
                            if samples_by_idx[i].len() < 5
                                && let Some(cell) = duckdb_to_cell_opt(v)
                            {
                                samples_by_idx[i].push(cell);
                            }
                        }
                    }
                }
            }

            // ── 4. Build column infos from SUMMARIZE output ───────────────────
            let columns: Vec<SchemaColumnInfo> = summarize_rows
                .into_iter()
                .enumerate()
                .map(|(i, (col_name, col_type, min_str, max_str))| {
                    let min = min_str
                        .as_deref()
                        .and_then(|s| parse_summarize_cell(s, &col_type));
                    let max = max_str
                        .as_deref()
                        .and_then(|s| parse_summarize_cell(s, &col_type));
                    let sample_values = samples_by_idx.get(i).cloned().unwrap_or_default();
                    SchemaColumnInfo {
                        name: col_name,
                        data_type: col_type,
                        min,
                        max,
                        sample_values,
                    }
                })
                .collect();

            tables.push(SchemaTableInfo {
                name: table.clone(),
                columns,
            });
        }

        // ── 5. Auto-detect join keys ──────────────────────────────────────────
        let join_keys = detect_join_keys(&tables);

        Ok(SchemaInfo { tables, join_keys })
    }
}

// ── AsArrowConnector impl (feature = "arrow") ────────────────────────────────

#[cfg(feature = "arrow")]
#[async_trait]
impl crate::connector::AsArrowConnector for DuckDbConnector {
    async fn execute_query_arrow(
        &self,
        sql: &str,
    ) -> Result<crate::connector::ArrowQueryStream, ConnectorError> {
        let sql = normalize_sql(sql);
        let conn = self
            .conn
            .lock()
            .map_err(|e| ConnectorError::ConnectionError(format!("mutex poisoned: {e}")))?;

        let mut stmt = conn.prepare(sql).map_err(|e| ConnectorError::QueryFailed {
            sql: sql.to_string(),
            message: e.to_string(),
        })?;
        let arrow_iter = stmt
            .query_arrow([])
            .map_err(|e| ConnectorError::QueryFailed {
                sql: sql.to_string(),
                message: e.to_string(),
            })?;
        let schema = arrow_iter.get_schema();
        // Collect eagerly; `Statement` borrows from the connection and the
        // iterator cannot outlive the lock. This matches the eager collection
        // strategy used by `execute_query_full`.
        let batches: Vec<::arrow::array::RecordBatch> = arrow_iter.collect();
        drop(stmt);

        Ok(crate::connector::ArrowQueryStream {
            schema,
            batches: Box::pin(futures::stream::iter(batches.into_iter().map(Ok))),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_collapses_spaces_to_underscores() {
        assert_eq!(
            normalize_table_name("c20251018_lake_sonoma_100k copy"),
            "c20251018_lake_sonoma_100k_copy"
        );
        assert_eq!(normalize_table_name("my data file"), "my_data_file");
    }

    #[test]
    fn normalize_prefixes_leading_digit() {
        assert_eq!(
            normalize_table_name("20250816_tamalpa_headlands_50k"),
            "_20250816_tamalpa_headlands_50k"
        );
    }

    #[test]
    fn normalize_leaves_clean_names_intact() {
        assert_eq!(normalize_table_name("oxymart"), "oxymart");
        assert_eq!(normalize_table_name("orders_2024"), "orders_2024");
    }

    #[test]
    fn normalize_handles_hyphens_and_dots() {
        assert_eq!(normalize_table_name("my-table.v2"), "my_table_v2");
    }

    #[test]
    fn normalize_fallback_for_empty_slug() {
        assert_eq!(normalize_table_name(""), "unnamed");
        assert_eq!(normalize_table_name("---"), "unnamed");
    }
}
