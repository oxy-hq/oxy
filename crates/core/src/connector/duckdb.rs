use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use arrow::array::RecordBatch;
use arrow::datatypes::SchemaRef;
use df_interchange::Interchange;
use duckdb::Connection;
use slugify::slugify;

use super::duckdb_pool::{PoolKey, pool};
use super::engine::Engine;
use crate::adapters::secrets::SecretsManager;
use crate::config::model::DuckDBOptions;
use crate::connector::constants::{
    CREATE_CONN, CREATE_TEMP_TABLE, EXECUTE_QUERY, PREPARE_DUCKDB_STMT, SET_FILE_SEARCH_PATH,
    SET_TEMP_DIRECTORY,
};
use crate::connector::utils::connector_internal_error;
use oxy_shared::errors::OxyError;

#[derive(Debug)]
pub(super) struct DuckDB {
    options: DuckDBOptions,
    secrets_manager: SecretsManager,
}

impl DuckDB {
    pub fn new(options: DuckDBOptions, secrets_manager: SecretsManager) -> Self {
        DuckDB {
            options,
            secrets_manager,
        }
    }

    /// Hand out a connection ready to run a query.
    ///
    /// For `Local` and `File` modes, the heavy work (opening the DB, loading
    /// CSVs into tables, installing extensions) happens exactly once per
    /// `(target, file mtimes)` key in [`super::duckdb_pool`]. Subsequent
    /// calls return a cheap `try_clone()` that shares the cached database.
    ///
    /// For `DuckLake` mode the per-call attach statements are derived from
    /// runtime secrets, so we keep the historical "fresh connection per
    /// query" behavior to avoid serving stale credentials.
    ///
    /// The non-DuckLake paths are wrapped in [`tokio::task::spawn_blocking`]
    /// because DuckDB's Rust binding is fully synchronous (statement
    /// preparation, CSV scans, file mtime stats). Running them on the async
    /// runtime would block worker threads — particularly painful on a
    /// busy `oxy serve` process where one slow CSV import would stall every
    /// other future on the same worker.
    pub async fn init_connection(&self) -> Result<Connection, OxyError> {
        match &self.options {
            DuckDBOptions::Local { file_search_path } => {
                let path = file_search_path.clone();
                tokio::task::spawn_blocking(move || checkout_local_blocking(&path))
                    .await
                    .map_err(|e| OxyError::DBError(format!("DuckDB checkout join error: {e}")))?
            }
            DuckDBOptions::File { path } => {
                let path = path.clone();
                tokio::task::spawn_blocking(move || checkout_file_blocking(&path))
                    .await
                    .map_err(|e| OxyError::DBError(format!("DuckDB checkout join error: {e}")))?
            }
            DuckDBOptions::DuckLake(_) => self.init_ducklake().await,
        }
    }

    async fn init_ducklake(&self) -> Result<Connection, OxyError> {
        let DuckDBOptions::DuckLake(config) = &self.options else {
            unreachable!("init_ducklake called with non-DuckLake options");
        };
        // Async: fetch secrets before entering spawn_blocking.
        let attach_stmts = config.to_duckdb_attach_stmt(&self.secrets_manager).await?;
        tracing::info!("Executing DuckDB attach statements: {:?}", attach_stmts);
        tokio::task::spawn_blocking(move || init_ducklake_blocking(attach_stmts))
            .await
            .map_err(|e| OxyError::DBError(format!("DuckDB ducklake join error: {e}")))?
    }
}

/// Synchronous body of [`DuckDB::init_ducklake`]. Runs inside
/// `spawn_blocking` to avoid blocking Tokio workers during `INSTALL`
/// (which may fetch extensions from the network on first run).
fn init_ducklake_blocking(attach_stmts: Vec<String>) -> Result<Connection, OxyError> {
    let conn =
        Connection::open_in_memory().map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
    conn.execute("INSTALL ducklake", [])
        .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
    conn.execute("LOAD ducklake", [])
        .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
    conn.execute("INSTALL postgres", [])
        .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
    conn.execute("LOAD postgres", [])
        .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
    for stmt in &attach_stmts {
        tracing::debug!("Executing DuckDB statement: {}", stmt);
        conn.execute(stmt, [])
            .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
    }
    install_icu(&conn)?;
    load_icu(&conn)?;
    Ok(conn)
}

/// Synchronous body of [`DuckDB::init_connection`] for `Local` mode. Lives
/// outside the async fn so it can run inside `spawn_blocking`.
fn checkout_local_blocking(file_search_path: &str) -> Result<Connection, OxyError> {
    let canonical_dir = canonicalize_local_dir(file_search_path)?;
    let files = collect_supported_files(&canonical_dir)?;
    if files.is_empty() {
        return Err(OxyError::DBError(format!(
            "DuckDB directory '{}' contains no .csv or .parquet files. Add at least one supported file or point to a different directory.",
            canonical_dir.display()
        )));
    }

    let key = PoolKey::local(canonical_dir.clone(), &files)?;
    let canonical_str = canonical_dir.display().to_string();
    let entry = pool().get_or_init(key, || {
        let conn = init_local_db(&canonical_dir, &files)?;
        // Re-run on every clone: cloned connections get a fresh session
        // and don't inherit `file_search_path`, `temp_directory`, or the
        // `LOAD icu` from the primary.
        let setup = vec![
            format!(
                "SET file_search_path = '{}'",
                escape_sql_string(&canonical_str)
            ),
            format!(
                "SET temp_directory = '{}'",
                escape_sql_string(&format!("{canonical_str}/tmp"))
            ),
            "LOAD icu".to_string(),
        ];
        Ok((conn, setup))
    })?;
    entry.checkout()
}

/// Synchronous body of [`DuckDB::init_connection`] for `File` mode.
fn checkout_file_blocking(path: &str) -> Result<Connection, OxyError> {
    let key = PoolKey::file(PathBuf::from(path))?;
    let path_owned = path.to_owned();
    let entry = pool().get_or_init(key, move || {
        let conn = Connection::open(&path_owned)
            .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
        install_icu(&conn)?;
        // `LOAD icu` is per-session; re-run on every clone.
        Ok((conn, vec!["LOAD icu".to_string()]))
    })?;
    entry.checkout()
}

/// First-time initialization for a `Local` mode database. Builds an
/// in-memory DuckDB pre-loaded with one regular (non-temporary) table per
/// file in `dir` so cloned connections from the pool see them. Tables are
/// `CREATE TABLE` rather than `CREATE TEMPORARY TABLE` because temp tables
/// are session-local and would be invisible to cloned connections.
fn init_local_db(
    canonical_dir: &Path,
    files: &[(String, PathBuf)],
) -> Result<Connection, OxyError> {
    let conn =
        Connection::open_in_memory().map_err(|err| connector_internal_error(CREATE_CONN, &err))?;

    let canonical_str = canonical_dir.display().to_string();
    let dir_set_stmt = format!(
        "SET file_search_path = '{}';",
        escape_sql_string(&canonical_str)
    );
    conn.execute(&dir_set_stmt, [])
        .map_err(|err| connector_internal_error(SET_FILE_SEARCH_PATH, &err))?;
    let temp_set_stmt = format!(
        "SET temp_directory = '{}';",
        escape_sql_string(&format!("{canonical_str}/tmp"))
    );
    conn.execute(&temp_set_stmt, [])
        .map_err(|err| connector_internal_error(SET_TEMP_DIRECTORY, &err))?;

    for (stem, path) in files {
        let table_name = slugify!(stem, separator = "_");
        let path_display = path.display().to_string();
        let create_stmt = format!(
            "CREATE TABLE {} AS FROM '{}'",
            quote_sql_identifier(&table_name),
            escape_sql_string(&path_display)
        );
        tracing::info!(
            "Creating pooled table '{}' from file '{}'",
            table_name,
            path_display
        );
        conn.execute(&create_stmt, [])
            .map_err(|err| connector_internal_error(CREATE_TEMP_TABLE, &err))?;
    }

    install_icu(&conn)?;
    tracing::debug!(
        "Initialized pooled DuckDB with file search path '{}'",
        canonical_str,
    );
    Ok(conn)
}

fn install_icu(conn: &Connection) -> Result<(), OxyError> {
    conn.execute("INSTALL icu", [])
        .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
    Ok(())
}

fn load_icu(conn: &Connection) -> Result<(), OxyError> {
    conn.execute("LOAD icu", [])
        .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
    Ok(())
}

/// Escape single quotes for inclusion inside a DuckDB single-quoted string literal.
fn escape_sql_string(s: &str) -> String {
    s.replace('\'', "''")
}

/// Wrap `name` in double quotes, escaping any embedded double quotes per SQL rules.
fn quote_sql_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Canonicalize `file_search_path` and verify it points to an existing directory.
fn canonicalize_local_dir(file_search_path: &str) -> Result<PathBuf, OxyError> {
    let path = Path::new(file_search_path);
    let canonical = path.canonicalize().map_err(|e| {
        OxyError::DBError(format!(
            "DuckDB path '{file_search_path}' does not exist or is not accessible: {e}"
        ))
    })?;
    if !canonical.is_dir() {
        return Err(OxyError::DBError(format!(
            "DuckDB path '{}' must be a directory containing .csv or .parquet files.",
            canonical.display()
        )));
    }
    Ok(canonical)
}

/// Scan `dir` (non-recursively) for `.csv` and `.parquet` files and return
/// `(file_stem, path)` pairs sorted by stem for deterministic output.
///
/// When two files share a stem (e.g. `orders.csv` and `orders.parquet`), only
/// the `.parquet` file is returned.
fn collect_supported_files(dir: &Path) -> Result<Vec<(String, PathBuf)>, OxyError> {
    let entries = std::fs::read_dir(dir).map_err(|e| {
        OxyError::DBError(format!(
            "Cannot read DuckDB directory '{}': {e}",
            dir.display()
        ))
    })?;

    // BTreeMap gives deterministic iteration order by stem name.
    let mut candidates: BTreeMap<String, (PathBuf, bool)> = BTreeMap::new();
    for entry_result in entries {
        let entry = match entry_result {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!(
                    "Skipping unreadable entry in DuckDB directory '{}': {err}",
                    dir.display()
                );
                continue;
            }
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
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

    Ok(candidates
        .into_iter()
        .map(|(stem, (path, _))| (stem, path))
        .collect())
}

impl Engine for DuckDB {
    async fn run_query_with_limit(
        &self,
        query: &str,
        _dry_run_limit: Option<u64>,
    ) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        let query = query.to_string();

        let conn = self.init_connection().await?;
        let mut stmt = conn
            .prepare(&query)
            .map_err(|err| connector_internal_error(PREPARE_DUCKDB_STMT, &err))?;
        let arrow_stream = stmt
            .query_arrow([])
            .map_err(|err| connector_internal_error(EXECUTE_QUERY, &err))?;
        let duckdb_chunks: Vec<_> = arrow_stream.collect();
        tracing::debug!("Query results: {:?}", duckdb_chunks);
        let arrow_chunks = Interchange::from_arrow_58(duckdb_chunks)
            .map_err(|err| connector_internal_error(EXECUTE_QUERY, &err))?
            .to_arrow_58()
            .map_err(|err| connector_internal_error(EXECUTE_QUERY, &err))?;
        let schema: SchemaRef = arrow_chunks
            .first()
            .map(|b| b.schema())
            .unwrap_or_else(|| std::sync::Arc::new(arrow::datatypes::Schema::empty()));
        Ok((arrow_chunks, schema))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_file(dir: &Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn escape_sql_string_doubles_single_quotes() {
        assert_eq!(escape_sql_string("no quotes"), "no quotes");
        assert_eq!(escape_sql_string("O'Brien"), "O''Brien");
        assert_eq!(escape_sql_string("'a'b'"), "''a''b''");
    }

    #[test]
    fn quote_sql_identifier_wraps_and_escapes_double_quotes() {
        assert_eq!(quote_sql_identifier("orders"), "\"orders\"");
        assert_eq!(quote_sql_identifier("weird\"name"), "\"weird\"\"name\"");
        assert_eq!(quote_sql_identifier(""), "\"\"");
    }

    #[test]
    fn canonicalize_local_dir_accepts_valid_directory() {
        let tmp = TempDir::new().unwrap();
        let canonical = canonicalize_local_dir(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(canonical, tmp.path().canonicalize().unwrap());
    }

    #[test]
    fn canonicalize_local_dir_rejects_nonexistent_path() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("does_not_exist");
        let err = canonicalize_local_dir(missing.to_str().unwrap()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("does not exist") || msg.contains("not accessible"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn canonicalize_local_dir_rejects_file() {
        let tmp = TempDir::new().unwrap();
        let file = write_file(tmp.path(), "orders.csv", "a,b\n1,2\n");
        let err = canonicalize_local_dir(file.to_str().unwrap()).unwrap_err();
        assert!(
            err.to_string().contains("must be a directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn collect_supported_files_returns_csv_and_parquet() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "orders.csv", "a,b\n1,2\n");
        write_file(tmp.path(), "customers.parquet", "");
        write_file(tmp.path(), "readme.md", "ignore me");
        write_file(tmp.path(), ".DS_Store", "");

        let files = collect_supported_files(tmp.path()).unwrap();
        let stems: Vec<&str> = files.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(stems, vec!["customers", "orders"]);
    }

    #[test]
    fn collect_supported_files_prefers_parquet_on_collision() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "orders.csv", "a,b\n1,2\n");
        let parquet = write_file(tmp.path(), "orders.parquet", "");

        let files = collect_supported_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, "orders");
        assert_eq!(files[0].1, parquet);
    }

    #[test]
    fn collect_supported_files_is_case_insensitive_on_extension() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "orders.CSV", "a,b\n1,2\n");
        write_file(tmp.path(), "customers.PARQUET", "");

        let files = collect_supported_files(tmp.path()).unwrap();
        let stems: Vec<&str> = files.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(stems, vec!["customers", "orders"]);
    }

    #[test]
    fn collect_supported_files_ignores_subdirectories() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("nested");
        fs::create_dir(&subdir).unwrap();
        write_file(&subdir, "deep.csv", "a,b\n1,2\n");
        write_file(tmp.path(), "top.csv", "a,b\n1,2\n");

        let files = collect_supported_files(tmp.path()).unwrap();
        let stems: Vec<&str> = files.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(stems, vec!["top"]);
    }

    #[test]
    fn collect_supported_files_returns_empty_for_dir_without_matches() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "notes.txt", "hi");
        write_file(tmp.path(), "data.json", "{}");

        let files = collect_supported_files(tmp.path()).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn collect_supported_files_sorted_deterministically() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "zeta.csv", "");
        write_file(tmp.path(), "alpha.csv", "");
        write_file(tmp.path(), "mike.csv", "");

        let files = collect_supported_files(tmp.path()).unwrap();
        let stems: Vec<&str> = files.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(stems, vec!["alpha", "mike", "zeta"]);
    }
}
