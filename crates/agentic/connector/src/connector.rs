//! Database connector abstraction.
//!
//! The FSM sends SQL to a [`DatabaseConnector`] and gets back bounded
//! results + summary stats in a single call. The database does the heavy
//! lifting. Rust holds only a capped sample.
//!
//! # Schema introspection
//!
//! Every connector that supports it can implement [`DatabaseConnector::introspect_schema`]
//! to return a vendor-neutral [`SchemaInfo`].  Callers (e.g. `AgentConfig::build_solver`)
//! use this to populate a `SchemaCatalog` with real column types, MIN/MAX bounds,
//! and sample values without knowing which database is behind the trait object.
//! Connectors that do not implement it return an empty [`SchemaInfo`] by default.

use async_trait::async_trait;
use std::fmt;

use agentic_core::result::{CellValue, QueryResult};

// ── Dialect ───────────────────────────────────────────────────────────────────

/// The SQL dialect spoken by a connector.
///
/// Used by the solver to inject dialect-specific instructions into the LLM
/// system prompt (e.g. "Use DuckDB SQL syntax").  Each connector returns its
/// own variant from [`DatabaseConnector::dialect`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlDialect {
    DuckDb,
    Sqlite,
    Postgres,
    BigQuery,
    Snowflake,
    /// Any vendor not covered by the variants above.  The inner string is a
    /// human-readable label used only in prompts.
    Other(&'static str),
}

impl SqlDialect {
    /// A concise, human-readable name for prompt injection.
    pub fn as_str(self) -> &'static str {
        match self {
            SqlDialect::DuckDb => "DuckDB",
            SqlDialect::Sqlite => "SQLite",
            SqlDialect::Postgres => "PostgreSQL",
            SqlDialect::BigQuery => "BigQuery",
            SqlDialect::Snowflake => "Snowflake",
            SqlDialect::Other(s) => s,
        }
    }
}

impl fmt::Display for SqlDialect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Schema introspection types ────────────────────────────────────────────────

/// Metadata about a single column as reported by the database.
#[derive(Debug, Clone, Default)]
pub struct SchemaColumnInfo {
    /// Column name (original case, as returned by the database).
    pub name: String,
    /// Database-native type string (e.g. `"INTEGER"`, `"VARCHAR"`, `"DOUBLE"`).
    pub data_type: String,
    /// Minimum value in this column (`None` if unavailable or all-NULL).
    pub min: Option<CellValue>,
    /// Maximum value in this column (`None` if unavailable or all-NULL).
    pub max: Option<CellValue>,
    /// Up to 5 distinct non-NULL sample values from this column.
    pub sample_values: Vec<CellValue>,
}

/// Metadata about a single table or view as reported by the database.
#[derive(Debug, Clone, Default)]
pub struct SchemaTableInfo {
    /// Table or view name (original case).
    pub name: String,
    pub columns: Vec<SchemaColumnInfo>,
}

/// Full database schema description returned by [`DatabaseConnector::introspect_schema`].
///
/// This is a vendor-neutral representation that callers convert into their own
/// catalog types (e.g. `SchemaCatalog::from_schema_info`).
#[derive(Debug, Clone, Default)]
pub struct SchemaInfo {
    pub tables: Vec<SchemaTableInfo>,
    /// Auto-detected or pre-declared join keys: `(table_a, table_b, join_column)`.
    pub join_keys: Vec<(String, String, String)>,
}

/// Per-column aggregate statistics computed by the database.
#[derive(Debug, Clone)]
pub struct ColumnStats {
    pub name: String,
    /// Database-native type name (e.g. "INTEGER", "VARCHAR", "TIMESTAMP").
    /// `None` when the connector cannot determine the type.
    pub data_type: Option<String>,
    pub null_count: u64,
    pub distinct_count: Option<u64>,
    pub min: Option<CellValue>,
    pub max: Option<CellValue>,
    pub mean: Option<f64>,
    pub std_dev: Option<f64>,
}

/// Summary statistics for a query result, computed by the database.
#[derive(Debug, Clone)]
pub struct ResultSummary {
    pub row_count: u64,
    pub columns: Vec<ColumnStats>,
}

/// Combined result of a connector execution: bounded rows + stats.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Bounded sample of rows.
    pub result: QueryResult,
    /// Per-column statistics computed by the database.
    pub summary: ResultSummary,
}

/// Errors from connector operations.
#[derive(Debug)]
pub enum ConnectorError {
    QueryFailed { sql: String, message: String },
    ConnectionError(String),
    Other(String),
}

impl fmt::Display for ConnectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QueryFailed { sql, message } => write!(f, "query failed: {message}\nSQL: {sql}"),
            Self::ConnectionError(msg) => write!(f, "connection error: {msg}"),
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for ConnectorError {}

/// Abstraction over database/warehouse query execution.
///
/// A single `execute_query` call returns bounded rows AND summary stats.
/// The connector decides how to do this efficiently — options include:
/// - Temp table: `CREATE TEMP TABLE _t AS (sql)`, then query _t for sample + stats
/// - Two queries: COUNT + LIMIT (acceptable for fast queries)
/// - Single pass with cursor: stream rows, compute stats incrementally, stop at limit
///
/// Connectors MUST enforce `sample_limit` — never return unbounded rows.
///
/// Connectors that support schema discovery should also implement
/// [`introspect_schema`] so callers can build a [`SchemaInfo`] without knowing
/// the underlying database technology.
///
/// [`introspect_schema`]: DatabaseConnector::introspect_schema
#[async_trait]
pub trait DatabaseConnector: Send + Sync {
    /// The SQL dialect this connector speaks.
    ///
    /// Used by the solver to inject dialect-specific instructions into the LLM
    /// prompts.  Every implementation must return a stable value — the solver
    /// reads it once at query time and does not cache it separately.
    fn dialect(&self) -> SqlDialect;

    /// Execute `sql`, return bounded rows + summary stats.
    ///
    /// `sample_limit`: max rows to include in `result.rows`.
    /// `result.total_row_count` must reflect the actual full count.
    /// `summary` must cover the full result set, not just the sample.
    async fn execute_query(
        &self,
        sql: &str,
        sample_limit: u64,
    ) -> Result<ExecutionResult, ConnectorError>;

    /// Return a vendor-neutral description of the database schema.
    ///
    /// The default implementation returns an empty [`SchemaInfo`] so
    /// connectors that do not support introspection remain valid trait
    /// objects.  Connectors that do support it should override this method
    /// and return tables, columns, types, MIN/MAX bounds, and sample values.
    fn introspect_schema(&self) -> Result<SchemaInfo, ConnectorError> {
        Ok(SchemaInfo::default())
    }
}
