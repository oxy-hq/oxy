//! Vendor-neutral connector configuration.
//!
//! [`ConnectorConfig`] describes *which* database/warehouse to connect to and
//! any connection-level parameters.  It contains no connector-crate types so
//! that this crate can be used without pulling in driver dependencies directly.
//!
//! `build_connector` reads this config and constructs a `Box<dyn DatabaseConnector>`.

use std::path::PathBuf;

// ── DuckDB ────────────────────────────────────────────────────────────────────

/// Controls whether DuckDB loads files lazily (view) or eagerly (materialized).
#[derive(Debug, Clone, Default)]
pub enum DuckDbLoadStrategy {
    /// `CREATE TEMP VIEW` — zero memory, re-reads the file on each query.
    #[default]
    View,
    /// `CREATE TEMP TABLE AS SELECT *` — materialized in DuckDB's in-process memory.
    Materialized,
}

/// Configuration for a local DuckDB connector backed by CSV/Parquet files in a
/// directory.
#[derive(Debug, Clone)]
pub struct DuckDbConfig {
    /// Directory that contains the CSV / Parquet files to register as tables.
    pub data_dir: PathBuf,
    /// Whether files are loaded as views (lazy) or materialized temp tables.
    pub load_strategy: DuckDbLoadStrategy,
}

// ── Postgres / Redshift ───────────────────────────────────────────────────────

/// Already-resolved connection parameters for a PostgreSQL (or Redshift) database.
#[derive(Debug, Clone)]
pub struct PostgresConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
}

// ── ClickHouse ────────────────────────────────────────────────────────────────

/// Already-resolved connection parameters for a ClickHouse HTTP endpoint.
#[derive(Debug, Clone)]
pub struct ClickHouseConfig {
    /// Full HTTP URL, e.g. `http://localhost:8123`.
    pub url: String,
    pub user: String,
    pub password: String,
    pub database: String,
}

// ── Snowflake ─────────────────────────────────────────────────────────────────

/// Already-resolved connection parameters for Snowflake (password auth).
#[derive(Debug, Clone)]
pub struct SnowflakeConfig {
    pub account: String,
    pub username: String,
    pub password: String,
    pub role: Option<String>,
    pub warehouse: String,
    pub database: Option<String>,
    pub schema: Option<String>,
}

// ── BigQuery ──────────────────────────────────────────────────────────────────

/// Already-resolved connection parameters for Google BigQuery.
#[derive(Debug, Clone)]
pub struct BigQueryConfig {
    /// Path to a service-account JSON key file.
    pub key_path: String,
    pub project_id: String,
    pub dataset: Option<String>,
}

// ── Top-level enum ────────────────────────────────────────────────────────────

/// Which database/warehouse to connect to and how.
///
/// Constructed by the caller (e.g. route handler) after secret resolution and
/// handed to `build_connector()`, which returns a `Box<dyn DatabaseConnector>`.
#[derive(Debug, Clone)]
pub enum ConnectorConfig {
    DuckDb(DuckDbConfig),
    Postgres(PostgresConfig),
    ClickHouse(ClickHouseConfig),
    Snowflake(SnowflakeConfig),
    BigQuery(BigQueryConfig),
}

impl ConnectorConfig {
    /// Convenience constructor: DuckDB in `View` mode pointed at `data_dir`.
    pub fn duckdb_from_dir(data_dir: impl Into<PathBuf>) -> Self {
        Self::DuckDb(DuckDbConfig {
            data_dir: data_dir.into(),
            load_strategy: DuckDbLoadStrategy::View,
        })
    }
}
