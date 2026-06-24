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
use crate::config::model::{DuckDBOptions, DuckDbS3Mirror};
use crate::connector::constants::{
    CREATE_CONN, CREATE_TEMP_TABLE, EXECUTE_QUERY, PREPARE_DUCKDB_STMT, SET_FILE_SEARCH_PATH,
    SET_TEMP_DIRECTORY,
};
use crate::connector::utils::connector_internal_error;
use oxy_shared::errors::OxyError;

#[derive(Debug)]
pub(super) struct DuckDB {
    options: DuckDBOptions,
    /// Compiled-config-only: present on a stateless replica when the local data
    /// was mirrored to S3 at compile time. Its presence means "read from S3";
    /// local/IDE reads never carry it.
    s3_mirror: Option<DuckDbS3Mirror>,
    secrets_manager: SecretsManager,
}

impl DuckDB {
    pub fn new(
        options: DuckDBOptions,
        s3_mirror: Option<DuckDbS3Mirror>,
        secrets_manager: SecretsManager,
    ) -> Self {
        DuckDB {
            options,
            s3_mirror,
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
        // Stateless-fleet read: the compile worker mirrored this workspace's
        // local DuckDB data to S3, so the local path doesn't exist here. Read it
        // over httpfs instead. (config.yml never carries a mirror, so local/IDE
        // reads fall through to the normal Local/File paths below.)
        if let Some(mirror) = &self.s3_mirror {
            let stmts = build_s3_mirror_sql(mirror);
            return tokio::task::spawn_blocking(move || init_s3_mirror_blocking(stmts))
                .await
                .map_err(|e| OxyError::DBError(format!("DuckDB s3-mirror join error: {e}")))?;
        }
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

/// Build the connection-setup SQL for an S3-mirrored DuckDB: load `httpfs`,
/// create the S3 secret (pod credential chain — no stored keys), then register
/// each mirrored data file as a view (`Local` mode) or attach the mirrored
/// database read-only (`File` mode). The views read Parquet/CSV lazily from S3,
/// so DuckDB still pushes projections/filters down rather than downloading
/// whole objects.
pub fn build_s3_mirror_sql(mirror: &DuckDbS3Mirror) -> Vec<String> {
    let mut stmts = vec!["INSTALL httpfs".to_string(), "LOAD httpfs".to_string()];

    let region = mirror.region.as_deref().unwrap_or("us-east-1");
    let mut secret = format!(
        "CREATE OR REPLACE SECRET duckdb_mirror_s3 (TYPE s3, PROVIDER credential_chain, REGION '{}'",
        escape_sql_string(region)
    );
    if let Some(endpoint) = &mirror.endpoint_url {
        // Custom endpoint (MinIO / LocalStack) → path-style addressing. DuckDB's
        // S3 secret ENDPOINT is host[:port] WITHOUT a scheme — it prepends
        // http(s):// itself based on USE_SSL. The mirror records the SDK's
        // `AWS_ENDPOINT_URL` verbatim (e.g. `http://localhost:9000`), so strip the
        // scheme here; otherwise DuckDB builds `http://http://localhost:9000` and
        // fails with "Could not resolve hostname", which silently drops the
        // connector and surfaces as "no databases configured".
        let use_ssl = !endpoint.starts_with("http://");
        let host = endpoint
            .strip_prefix("http://")
            .or_else(|| endpoint.strip_prefix("https://"))
            .unwrap_or(endpoint)
            .trim_end_matches('/');
        secret.push_str(&format!(
            ", ENDPOINT '{}', URL_STYLE 'path', USE_SSL {}",
            escape_sql_string(host),
            use_ssl
        ));
    }
    secret.push(')');
    stmts.push(secret);

    if let Some(key) = &mirror.attach_key {
        stmts.push(format!(
            "ATTACH 's3://{}/{}' AS mirror (READ_ONLY)",
            escape_sql_string(&mirror.bucket),
            escape_sql_string(key)
        ));
        stmts.push("USE mirror".to_string());
    }
    for table in &mirror.tables {
        let reader = if table.format.eq_ignore_ascii_case("csv") {
            "read_csv_auto"
        } else {
            "read_parquet"
        };
        let source = format!(
            "{}('s3://{}/{}')",
            reader,
            escape_sql_string(&mirror.bucket),
            escape_sql_string(&table.key)
        );
        // 1) Slug-named view (`oxymart`) — matches the local connector's pooled
        //    table registration; used by direct references / list_tables.
        stmts.push(format!(
            "CREATE OR REPLACE VIEW {} AS SELECT * FROM {source}",
            quote_sql_identifier(&table.table),
        ));
        // 2) Filename-qualified view. The semantic layer references the table by
        //    its source filename (`table: "oxymart.csv"`); in local mode that
        //    resolves through DuckDB's `file_search_path` replacement scan, which
        //    the stateless fleet has no equivalent for. DuckDB parses
        //    `FROM oxymart.csv` as schema.table, so register a matching
        //    `"<stem>"."<ext>"` view (stem = slug, ext = format), or `FROM
        //    oxymart.csv` finds nothing and fails "No files found".
        stmts.push(format!(
            "CREATE SCHEMA IF NOT EXISTS {}",
            quote_sql_identifier(&table.table)
        ));
        stmts.push(format!(
            "CREATE OR REPLACE VIEW {}.{} AS SELECT * FROM {source}",
            quote_sql_identifier(&table.table),
            quote_sql_identifier(&table.format),
        ));
    }
    stmts
}

/// Synchronous body of the S3-mirror path. Fresh in-memory connection per
/// query (like DuckLake) — `INSTALL` is disk-cached after first run, and the
/// remaining `LOAD` / secret / view statements are cheap.
fn init_s3_mirror_blocking(stmts: Vec<String>) -> Result<Connection, OxyError> {
    let conn =
        Connection::open_in_memory().map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
    for stmt in &stmts {
        tracing::debug!("DuckDB s3-mirror stmt: {}", stmt);
        conn.execute(stmt, [])
            .map_err(|err| s3_mirror_setup_error(stmt, &err))?;
    }
    install_icu(&conn)?;
    load_icu(&conn)?;
    Ok(conn)
}

/// Turn a DuckDB error from an S3-mirror setup statement into a message that
/// names the step and what an operator should check. This path only runs on a
/// stateless replica serving a workspace whose local DuckDB data was mirrored
/// to S3 at compile time, so the failures are S3/extension-shaped, not the
/// usual "bad SQL".
fn s3_mirror_setup_error(stmt: &str, err: &duckdb::Error) -> OxyError {
    let hint = if stmt.contains("httpfs") {
        "could not load the DuckDB `httpfs` extension — the node needs outbound network to the \
         extension repository on first run"
    } else if stmt.starts_with("ATTACH") {
        "could not attach the S3-mirrored DuckDB file read-only — the bundled DuckDB may not \
         support remote ATTACH, the object may be missing, or the pod's IAM role may lack \
         s3:GetObject on the bucket"
    } else if stmt.contains("SECRET") {
        "could not create the S3 credential for the mirrored DuckDB warehouse (expected the pod's \
         instance role via the credential chain)"
    } else {
        "could not register an S3-mirrored DuckDB table — check the mirrored object exists and \
         the pod's IAM role can read it"
    };
    OxyError::DBError(format!("DuckDB S3 mirror: {hint}. Underlying error: {err}"))
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

/// Check out a connection to a DuckDB file database from the process-wide
/// pool, initialising it if this is the first call for `path`.
///
/// The returned connection shares the same underlying `duckdb_database`
/// handle as every other connection checked out from the pool for this path.
/// Callers (e.g. the airform integration) must use this instead of opening a
/// fresh `Connection::open(path)` to avoid having two independent
/// `duckdb_database` handles on the same file in the same process — a
/// situation that bypasses OS advisory locking and causes SIGSEGV in
/// DuckDB's native code.
pub fn checkout_file_connection(path: &str) -> Result<Connection, OxyError> {
    checkout_file_blocking(path)
}

/// Check out a pooled connection for a local directory database (CSV/Parquet
/// files). On the first call for `file_search_path` the files are loaded into
/// an in-memory DuckDB; subsequent calls return a cheap `try_clone()` of the
/// same primary connection. Mtime-based eviction ensures stale data is never
/// served after a file change.
pub fn checkout_local_connection(file_search_path: &str) -> Result<Connection, OxyError> {
    checkout_local_blocking(file_search_path)
}

/// Synchronous body of [`DuckDB::init_connection`] for `File` mode.
fn checkout_file_blocking(path: &str) -> Result<Connection, OxyError> {
    // `Connection::open` on a missing path silently CREATES an empty database,
    // which then fails every query with a confusing "table not found". Catch it
    // here with an actionable message instead — this is the common shape on a
    // stateless replica whose `.duckdb` wasn't mirrored to S3.
    if !Path::new(path).exists() {
        return Err(OxyError::DBError(format!(
            "DuckDB database file '{path}' was not found. If this is a stateless / cloud \
             deployment, the file must be mirrored to S3 by the compile worker — set \
             OXY_COMPILE_BLOB_S3_BUCKET and recompile (files over 256 MiB are skipped and stay \
             local-only). Otherwise check the path."
        )));
    }
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

        // Also expose the table under the full filename (e.g. "climbing.csv")
        // so semantic-layer views that declare `table: "climbing.csv"` resolve
        // against the in-memory table instead of triggering a per-query file
        // scan — which reads the CSV from disk every time.
        if let Some(full_name) = path.file_name().and_then(|n| n.to_str()) {
            let full_escaped = full_name.replace('"', "\"\"");
            if full_escaped != table_name {
                let alias_sql = format!(
                    "CREATE OR REPLACE VIEW {full_quoted} AS SELECT * FROM {table_quoted}",
                    full_quoted = quote_sql_identifier(full_name),
                    table_quoted = quote_sql_identifier(&table_name),
                );
                let _ = conn.execute(&alias_sql, []);
            }
        }
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
            "DuckDB dataset directory '{file_search_path}' was not found ({e}). If this is a \
             stateless / cloud deployment, the workspace's local DuckDB data must be mirrored to \
             S3 by the compile worker — set OXY_COMPILE_BLOB_S3_BUCKET and recompile (note files \
             over 256 MiB are skipped and stay local-only). Otherwise check that the path exists \
             and is readable."
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

        // S3-mirror views read Parquet/CSV from S3 lazily, so a missing object
        // or a credentials problem surfaces here at execution, not at setup.
        let s3_mirror = self.s3_mirror.is_some();
        let conn = self.init_connection().await?;
        let mut stmt = conn
            .prepare(&query)
            .map_err(|err| connector_internal_error(PREPARE_DUCKDB_STMT, &err))?;
        let arrow_stream = stmt.query_arrow([]).map_err(|err| {
            let base = connector_internal_error(EXECUTE_QUERY, &err);
            if s3_mirror {
                OxyError::DBError(format!(
                    "{base} — this DuckDB warehouse is served from S3 on this node; verify the \
                     mirrored objects exist and the pod's IAM role can read them (s3:GetObject)."
                ))
            } else {
                base
            }
        })?;
        let duckdb_chunks: Vec<_> = arrow_stream.collect();
        tracing::debug!("Query results: {:?}", duckdb_chunks);
        // `Interchange::from_arrow_58` indexes `df[0]` without an empty
        // guard (df-interchange-0.3.3/src/from_arrow.rs:19), so a
        // zero-chunk result — zero-row queries, some startup metadata
        // calls — panics. Return the empty-result shape directly.
        if duckdb_chunks.is_empty() {
            return Ok((
                Vec::new(),
                std::sync::Arc::new(arrow::datatypes::Schema::empty()),
            ));
        }
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
        assert!(msg.contains("was not found"), "unexpected error: {msg}");
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

    use crate::config::model::{DuckDbS3Mirror, DuckDbS3Table};

    #[test]
    fn s3_mirror_local_mode_builds_httpfs_secret_and_views() {
        let mirror = DuckDbS3Mirror {
            bucket: "my-bucket".into(),
            region: Some("us-west-2".into()),
            endpoint_url: None,
            tables: vec![
                DuckDbS3Table {
                    table: "orders".into(),
                    key: "ws/duckdb/orders.parquet".into(),
                    format: "parquet".into(),
                },
                DuckDbS3Table {
                    table: "events".into(),
                    key: "ws/duckdb/events.csv".into(),
                    format: "csv".into(),
                },
            ],
            attach_key: None,
        };
        let sql = build_s3_mirror_sql(&mirror);
        assert!(sql.contains(&"INSTALL httpfs".to_string()));
        assert!(sql.contains(&"LOAD httpfs".to_string()));
        assert!(
            sql.iter()
                .any(|s| s.contains("CREATE OR REPLACE SECRET duckdb_mirror_s3")
                    && s.contains("PROVIDER credential_chain")
                    && s.contains("REGION 'us-west-2'"))
        );
        assert!(sql.iter().any(|s| s.contains("CREATE OR REPLACE VIEW")
            && s.contains("orders")
            && s.contains("read_parquet('s3://my-bucket/ws/duckdb/orders.parquet')")));
        assert!(sql.iter().any(|s| s.contains("CREATE OR REPLACE VIEW")
            && s.contains("events")
            && s.contains("read_csv_auto('s3://my-bucket/ws/duckdb/events.csv')")));
        // Filename-qualified views so the semantic layer's `FROM orders.parquet`
        // / `FROM events.csv` (which DuckDB parses as schema.table) resolve to the
        // mirrored data — local mode gets this from file_search_path; the fleet
        // can't, so register `"<stem>"."<ext>"` explicitly.
        assert!(
            sql.iter()
                .any(|s| s == "CREATE SCHEMA IF NOT EXISTS \"orders\""),
            "missing schema for orders: {sql:?}"
        );
        assert!(
            sql.iter().any(
                |s| s.contains("CREATE OR REPLACE VIEW \"orders\".\"parquet\"")
                    && s.contains("read_parquet('s3://my-bucket/ws/duckdb/orders.parquet')")
            ),
            "missing filename-qualified view for orders.parquet: {sql:?}"
        );
        assert!(
            sql.iter()
                .any(|s| s == "CREATE SCHEMA IF NOT EXISTS \"events\""),
            "missing schema for events: {sql:?}"
        );
        assert!(
            sql.iter()
                .any(|s| s.contains("CREATE OR REPLACE VIEW \"events\".\"csv\"")
                    && s.contains("read_csv_auto('s3://my-bucket/ws/duckdb/events.csv')")),
            "missing filename-qualified view for events.csv: {sql:?}"
        );
        // Local mode never attaches a remote database.
        assert!(!sql.iter().any(|s| s.contains("ATTACH")));
    }

    #[test]
    fn s3_mirror_file_mode_attaches_read_only() {
        let mirror = DuckDbS3Mirror {
            bucket: "b".into(),
            region: None,
            endpoint_url: None,
            tables: vec![],
            attach_key: Some("ws/duckdb/data.duckdb".into()),
        };
        let sql = build_s3_mirror_sql(&mirror);
        assert!(
            sql.iter()
                .any(|s| s == "ATTACH 's3://b/ws/duckdb/data.duckdb' AS mirror (READ_ONLY)")
        );
        assert!(sql.contains(&"USE mirror".to_string()));
    }

    #[test]
    fn s3_mirror_custom_endpoint_uses_path_style() {
        let mirror = DuckDbS3Mirror {
            bucket: "b".into(),
            region: Some("us-east-1".into()),
            endpoint_url: Some("http://localhost:9000".into()),
            tables: vec![],
            attach_key: None,
        };
        let sql = build_s3_mirror_sql(&mirror);
        // The ENDPOINT must be host[:port] with NO scheme — DuckDB prepends
        // http(s):// from USE_SSL. Passing the scheme yields `http://http://…`,
        // which fails to resolve and silently drops the connector.
        assert!(
            sql.iter().any(|s| s.contains("ENDPOINT 'localhost:9000'")
                && s.contains("URL_STYLE 'path'")
                && s.contains("USE_SSL false")),
            "expected scheme-stripped endpoint, got: {sql:?}"
        );
        assert!(
            !sql.iter().any(|s| s.contains("ENDPOINT 'http://")),
            "endpoint must not carry a scheme: {sql:?}"
        );
    }

    #[test]
    fn s3_mirror_https_endpoint_strips_scheme_and_enables_ssl() {
        let mirror = DuckDbS3Mirror {
            bucket: "b".into(),
            region: Some("us-east-1".into()),
            endpoint_url: Some("https://minio.example.com/".into()),
            tables: vec![],
            attach_key: None,
        };
        let sql = build_s3_mirror_sql(&mirror);
        assert!(
            sql.iter()
                .any(|s| s.contains("ENDPOINT 'minio.example.com'") && s.contains("USE_SSL true")),
            "https endpoint should strip scheme + trailing slash and enable SSL: {sql:?}"
        );
    }
}
