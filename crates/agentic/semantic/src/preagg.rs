//! Thin wrappers over airlayer pre-aggregation functions and DuckDB execution.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use agentic_connector::DatabaseConnector;
use agentic_core::result::{CellValue, QueryResult, QueryRow};
use serde_json::Value;

use crate::error::SemanticError;

/// Process-wide primary connection used to seed cheap per-call clones for
/// preagg reads. Opening a fresh `Connection::open_in_memory()` per request
/// dominates the cache-hit query path (~30–40ms cold); `try_clone()` on a
/// long-lived primary brings that down to sub-millisecond.
///
/// The primary holds no schema and no preloaded tables — every preagg SQL
/// has its own `read_parquet('...')` call, so clones are independent.
static PRIMARY_DUCKDB: OnceLock<Mutex<duckdb::Connection>> = OnceLock::new();

/// Return a fresh DuckDB connection cloned from the long-lived primary.
/// Use this for any per-request DuckDB work tied to preagg reads so we
/// don't pay the in-memory-DB open cost on every query.
pub fn pooled_duckdb_connection() -> Result<duckdb::Connection, SemanticError> {
    let primary = PRIMARY_DUCKDB.get_or_init(|| {
        Mutex::new(
            duckdb::Connection::open_in_memory()
                .expect("open primary in-memory DuckDB connection for preagg pool"),
        )
    });
    let guard = primary
        .lock()
        .expect("preagg DuckDB primary mutex poisoned");
    guard
        .try_clone()
        .map_err(|e| SemanticError::Runtime(format!("DuckDB try_clone failed: {e}")))
}

/// Execute a re-aggregation SQL query against an in-memory DuckDB instance.
///
/// `preagg_sql` is produced by `airlayer::preagg::generate_preagg_sql` and references
/// the Parquet file via `read_parquet('...')`. We verify the file exists first to give
/// a clear error instead of a cryptic DuckDB message.
pub fn execute_preagg_sql(preagg_sql: &str, parquet_path: &Path) -> Result<Value, SemanticError> {
    if !parquet_path.is_file() {
        return Err(SemanticError::Runtime(format!(
            "Parquet cache file not found: {}",
            parquet_path.display()
        )));
    }

    let conn = pooled_duckdb_connection()?;

    let mut stmt = conn
        .prepare(preagg_sql)
        .map_err(|e| SemanticError::Runtime(format!("DuckDB prepare failed: {e}")))?;

    let mut duckdb_rows = stmt
        .query([])
        .map_err(|e| SemanticError::Runtime(format!("DuckDB query failed: {e}")))?;

    let columns: Vec<String> = duckdb_rows
        .as_ref()
        .map(|r| r.column_names().iter().map(|s| s.to_string()).collect())
        .unwrap_or_default();

    let mut json_rows: Vec<Value> = Vec::new();
    loop {
        match duckdb_rows.next() {
            Ok(Some(row)) => {
                let mut obj = serde_json::Map::new();
                for (i, col) in columns.iter().enumerate() {
                    obj.insert(col.clone(), row_value_to_json(row, i));
                }
                json_rows.push(Value::Object(obj));
            }
            Ok(None) => break,
            Err(e) => return Err(SemanticError::Runtime(format!("DuckDB row error: {e}"))),
        }
    }

    let row_count = json_rows.len();
    Ok(serde_json::json!({
        "columns": columns,
        "rows": json_rows,
        "row_count": row_count,
        "truncated": false,
    }))
}

/// Typed variant of [`execute_preagg_sql`] that returns `QueryRow`s directly —
/// no intermediate `serde_json::Value` per cell — and applies a sample limit
/// at the SQL level so DuckDB stops scanning early.
///
/// Returns `(columns, sample_rows, total_row_count)`. `sample_rows.len()` is
/// `min(total_row_count, sample_limit)`. Callers derive `truncated` by
/// comparing the two.
///
/// Total row count comes from a separate `SELECT COUNT(*) FROM (..)` against
/// the same preagg SQL — cheap on Parquet (DuckDB reads metadata) and avoids
/// materialising rows the caller would discard.
pub fn execute_preagg_sql_typed(
    preagg_sql: &str,
    parquet_path: &Path,
    sample_limit: u64,
) -> Result<(Vec<String>, Vec<QueryRow>, u64), SemanticError> {
    if !parquet_path.is_file() {
        return Err(SemanticError::Runtime(format!(
            "Parquet cache file not found: {}",
            parquet_path.display()
        )));
    }

    let conn = pooled_duckdb_connection()?;

    let count_sql = format!("SELECT COUNT(*) FROM ({preagg_sql})");
    let total_row_count = conn
        .query_row(&count_sql, [], |row| row.get::<_, i64>(0))
        .map_err(|e| SemanticError::Runtime(format!("DuckDB count failed: {e}")))?
        as u64;

    let limited_sql = format!("SELECT * FROM ({preagg_sql}) LIMIT {sample_limit}");
    let mut stmt = conn
        .prepare(&limited_sql)
        .map_err(|e| SemanticError::Runtime(format!("DuckDB prepare failed: {e}")))?;

    let mut duckdb_rows = stmt
        .query([])
        .map_err(|e| SemanticError::Runtime(format!("DuckDB query failed: {e}")))?;

    let columns: Vec<String> = duckdb_rows
        .as_ref()
        .map(|r| r.column_names().iter().map(|s| s.to_string()).collect())
        .unwrap_or_default();

    let capacity = sample_limit.min(total_row_count) as usize;
    let mut rows: Vec<QueryRow> = Vec::with_capacity(capacity);
    loop {
        match duckdb_rows.next() {
            Ok(Some(row)) => {
                let cells: Vec<CellValue> = (0..columns.len())
                    .map(|i| row_value_to_cell(row, i))
                    .collect();
                rows.push(QueryRow(cells));
            }
            Ok(None) => break,
            Err(e) => return Err(SemanticError::Runtime(format!("DuckDB row error: {e}"))),
        }
    }

    Ok((columns, rows, total_row_count))
}

fn row_value_to_cell(row: &duckdb::Row<'_>, idx: usize) -> CellValue {
    use duckdb::types::ValueRef;
    match row.get_ref_unwrap(idx) {
        ValueRef::Null => CellValue::Null,
        ValueRef::TinyInt(n) => CellValue::Number(n as f64),
        ValueRef::SmallInt(n) => CellValue::Number(n as f64),
        ValueRef::Int(n) => CellValue::Number(n as f64),
        ValueRef::BigInt(n) => CellValue::Number(n as f64),
        ValueRef::UTinyInt(n) => CellValue::Number(n as f64),
        ValueRef::USmallInt(n) => CellValue::Number(n as f64),
        ValueRef::UInt(n) => CellValue::Number(n as f64),
        ValueRef::UBigInt(n) => CellValue::Number(n as f64),
        ValueRef::HugeInt(n) => i64::try_from(n)
            .map(|v| CellValue::Number(v as f64))
            .unwrap_or_else(|_| CellValue::Text(n.to_string())),
        ValueRef::Float(f) => CellValue::Number(f as f64),
        ValueRef::Double(f) => CellValue::Number(f),
        ValueRef::Decimal(d) => {
            let s = d.to_string();
            s.parse::<f64>()
                .map(CellValue::Number)
                .unwrap_or(CellValue::Text(s))
        }
        ValueRef::Text(s) => CellValue::Text(String::from_utf8_lossy(s).into_owned()),
        ValueRef::Blob(b) => CellValue::Text(format!("<blob {} bytes>", b.len())),
        ValueRef::Boolean(b) => CellValue::Text(b.to_string()),
        other => CellValue::Text(format!("{other:?}")),
    }
}

fn row_value_to_json(row: &duckdb::Row<'_>, idx: usize) -> Value {
    use duckdb::types::ValueRef;
    match row.get_ref_unwrap(idx) {
        ValueRef::Null => Value::Null,
        ValueRef::TinyInt(n) => serde_json::json!(n),
        ValueRef::SmallInt(n) => serde_json::json!(n),
        ValueRef::Int(n) => serde_json::json!(n),
        ValueRef::BigInt(n) => serde_json::json!(n),
        ValueRef::UTinyInt(n) => serde_json::json!(n),
        ValueRef::USmallInt(n) => serde_json::json!(n),
        ValueRef::UInt(n) => serde_json::json!(n),
        ValueRef::UBigInt(n) => serde_json::json!(n),
        // SUM(int32) over a Parquet rollup comes back as HugeInt (i128) even
        // when the value fits easily in i64 — typical for count/sum rollups.
        // Emit as a JSON number when it fits so chart code that expects
        // numeric y-axis values works; fall back to string only for the
        // genuine overflow case.
        ValueRef::HugeInt(n) => i64::try_from(n)
            .map(|v| serde_json::json!(v))
            .unwrap_or_else(|_| Value::String(n.to_string())),
        ValueRef::Float(f) => serde_json::json!(f),
        ValueRef::Double(f) => serde_json::json!(f),
        // Decimal columns (Parquet `decimal(p,s)`) used to fall through the
        // catch-all and serialise as `"Decimal { ... }"` debug strings,
        // breaking every chart whose measure was a SUM/AVG over a money
        // column. Convert to f64 (good enough for chart rendering) and
        // string-fallback only when conversion fails.
        ValueRef::Decimal(d) => {
            let s = d.to_string();
            s.parse::<f64>()
                .ok()
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number)
                .unwrap_or(Value::String(s))
        }
        ValueRef::Text(s) => Value::String(String::from_utf8_lossy(s).into_owned()),
        ValueRef::Blob(b) => Value::String(format!("<blob {} bytes>", b.len())),
        ValueRef::Boolean(b) => serde_json::json!(b),
        other => Value::String(format!("{other:?}")),
    }
}

/// Execute all statements in a `BuildPlan` against a warehouse connector.
pub async fn execute_build_plan(
    connector: &Arc<dyn DatabaseConnector>,
    plan: &airlayer::preagg::BuildPlan,
) -> Result<(), SemanticError> {
    for stmt in &plan.statements {
        connector.execute_statement(stmt).await.map_err(|e| {
            SemanticError::Runtime(format!("build plan statement failed: {e}\nSQL: {stmt}"))
        })?;
    }
    Ok(())
}

/// Download a single pre-aggregation rollup from the warehouse into a Parquet file.
///
/// Returns `true` if the file was written, `false` if the rollup had no rows
/// (caller should skip [`hot_swap_parquet`] in that case).
/// Writes to `temp_path`. Call [`hot_swap_parquet`] after to atomically rename it
/// to the final path.
/// Maximum number of rows `pull_rollup` will materialise into the local
/// Parquet cache in a single pass. The current implementation buffers every
/// row into memory and serialises a single `VALUES (...)` SQL statement into
/// DuckDB, so anything beyond this would build a multi-GB SQL string and
/// likely OOM the worker. When this trips, the rollup is too large for the
/// current pipeline — surface a clear error so operators size their
/// pre-aggregations down or wait for the streaming Arrow/Appender rewrite.
pub const PULL_ROLLUP_ROW_LIMIT: u64 = 500_000;

pub async fn pull_rollup(
    connector: &Arc<dyn DatabaseConnector>,
    table_name: &str,
    temp_path: &Path,
) -> Result<bool, SemanticError> {
    let sql = format!("SELECT * FROM {table_name}");
    let result = connector
        .execute_query(&sql, PULL_ROLLUP_ROW_LIMIT)
        .await
        .map_err(|e| SemanticError::Runtime(format!("pull failed for {table_name}: {e}")))?;

    if result.result.truncated {
        return Err(SemanticError::Runtime(format!(
            "pull_rollup: rollup {table_name} exceeds PULL_ROLLUP_ROW_LIMIT ({PULL_ROLLUP_ROW_LIMIT} rows). \
             The current materializer buffers all rows into a single VALUES SQL statement; \
             reduce the rollup's grain or wait for the streaming Arrow rewrite."
        )));
    }

    write_result_to_parquet(&result.result, temp_path)
}

/// Atomically rename `temp_path` → `final_path`.
///
/// On POSIX systems `rename(2)` is atomic: readers of the old file are unaffected;
/// new readers get the new file.
pub fn hot_swap_parquet(temp_path: &Path, final_path: &Path) -> Result<(), SemanticError> {
    if let Some(parent) = final_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| SemanticError::Runtime(format!("create_dir_all failed: {e}")))?;
    }
    std::fs::rename(temp_path, final_path)
        .map_err(|e| SemanticError::Runtime(format!("hot-swap rename failed: {e}")))?;
    Ok(())
}

/// Returns `true` if the file was written, `false` if the result had no rows.
fn write_result_to_parquet(result: &QueryResult, path: &Path) -> Result<bool, SemanticError> {
    if result.rows.is_empty() {
        tracing::warn!(
            "pull_rollup: no rows to write to {}, skipping",
            path.display()
        );
        return Ok(false);
    }

    let conn = duckdb::Connection::open_in_memory().map_err(|e| {
        SemanticError::Runtime(format!("DuckDB open failed for parquet write: {e}"))
    })?;

    let col_names: Vec<String> = result
        .columns
        .iter()
        .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
        .collect();

    let mut value_rows: Vec<String> = Vec::new();
    for row in &result.rows {
        let cells: Vec<String> = row
            .0
            .iter()
            .map(|cell| match cell {
                CellValue::Text(s) => format!("'{}'", s.replace('\'', "''")),
                // NaN / Inf are not valid SQL literals — map them to NULL.
                CellValue::Number(n) if n.is_finite() => n.to_string(),
                CellValue::Number(_) => "NULL".into(),
                CellValue::Null => "NULL".into(),
            })
            .collect();
        value_rows.push(format!("({})", cells.join(", ")));
    }

    let path_str = path
        .to_str()
        .ok_or_else(|| SemanticError::Runtime("non-UTF8 parquet path".into()))?;

    let sql = format!(
        "COPY (SELECT * FROM (VALUES {}) AS t({})) TO '{}' (FORMAT PARQUET)",
        value_rows.join(", "),
        col_names.join(", "),
        path_str.replace('\'', "''"),
    );

    conn.execute_batch(&sql)
        .map_err(|e| SemanticError::Runtime(format!("parquet write failed: {e}")))?;
    Ok(true)
}

/// Load the local manifest from `<cache_dir>/manifest.json`.
///
/// Returns `None` if the file doesn't exist or can't be parsed.
pub fn load_local_manifest(cache_dir: &Path) -> Option<airlayer::preagg::LocalManifest> {
    let path = cache_dir.join("manifest.json");
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Persist a local manifest to `<cache_dir>/manifest.json` atomically.
///
/// Writes to a `.tmp` file first, then renames it into place. On POSIX systems
/// `rename(2)` is atomic: readers of the old file are unaffected; new readers
/// get the new file. This prevents partial writes if the process is killed mid-write.
pub fn save_local_manifest(
    cache_dir: &Path,
    manifest: &airlayer::preagg::LocalManifest,
) -> Result<(), SemanticError> {
    std::fs::create_dir_all(cache_dir)
        .map_err(|e| SemanticError::Runtime(format!("create cache dir failed: {e}")))?;

    let tmp_path = cache_dir.join("manifest.json.tmp");
    let final_path = cache_dir.join("manifest.json");

    let json = serde_json::to_string_pretty(manifest)
        .map_err(|e| SemanticError::Runtime(format!("manifest serialize failed: {e}")))?;

    std::fs::write(&tmp_path, &json)
        .map_err(|e| SemanticError::Runtime(format!("manifest tmp write failed: {e}")))?;

    std::fs::rename(&tmp_path, &final_path)
        .map_err(|e| SemanticError::Runtime(format!("manifest rename failed: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn save_and_load_manifest_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = airlayer::preagg::LocalManifest {
            pulled_at: "2026-01-01T00:00:00Z".into(),
            source_database: "my_db".into(),
            rollups: vec![],
        };
        save_local_manifest(dir.path(), &manifest).unwrap();

        let loaded = load_local_manifest(dir.path());
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().source_database, "my_db");
    }

    #[test]
    fn save_manifest_no_tmp_remains() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = airlayer::preagg::LocalManifest {
            pulled_at: "2026-01-01T00:00:00Z".into(),
            source_database: "test".into(),
            rollups: vec![],
        };
        save_local_manifest(dir.path(), &manifest).unwrap();

        // No .tmp file should remain after a successful save.
        assert!(!dir.path().join("manifest.json.tmp").exists());
        assert!(dir.path().join("manifest.json").exists());
    }

    #[test]
    fn test_execute_preagg_sql_parquet_not_found() {
        let result = execute_preagg_sql(
            "SELECT * FROM read_parquet('/does/not/exist.parquet')",
            &PathBuf::from("/does/not/exist.parquet"),
        );
        assert!(result.is_err(), "missing parquet should error");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not found"),
            "error should mention 'not found': {err}"
        );
    }
}
