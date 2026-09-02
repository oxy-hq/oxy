//! Semantic query compilation abstraction for the builder domain.
//!
//! The builder solver invokes semantic compilation through this trait so
//! the pipeline layer can supply the concrete implementation (typically
//! `agentic_automation::semantic_bridge`).

use agentic_core::result::QueryResult;
use agentic_core::tools::ToolError;
use async_trait::async_trait;

/// Opaque identifier for where a rollup's data lives, minted by the compiler
/// and handed straight back to [`BuilderSemanticCompiler::execute_preagg`].
///
/// Deliberately opaque. It used to be a `PathBuf`, which was fine while the
/// only answer was "a file on this node" and wrong as soon as a rollup could
/// also be read from the blob store. This crate does not depend on
/// `agentic-semantic` — that separation is the point of this trait — so it
/// carries the implementer's own encoding rather than mirroring its type, and
/// nothing here interprets it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreaggHandle(pub String);

/// Result of compiling a semantic query.
pub enum SemanticCompilationResult {
    /// Compiled to warehouse SQL. Run against the named database connector.
    Warehouse { sql: String, database_name: String },
    /// Pre-aggregation cache hit. `preagg_sql` reads the rollup named by
    /// `handle` via an in-process DuckDB instance — bypasses the warehouse
    /// connector. `warehouse_sql` is the SQL that would have run against the
    /// warehouse, surfaced so users and the agent see the logical query rather
    /// than the DuckDB rewrite.
    Preaggregation {
        preagg_sql: String,
        handle: PreaggHandle,
        warehouse_sql: String,
        warehouse_database: String,
    },
}

/// Compiles semantic model queries to SQL.
///
/// The builder domain uses this trait instead of depending on `oxy-workflow`
/// and `oxy-semantic` directly. The pipeline layer supplies the implementation
/// that bridges to the oxy semantic validation and compilation pipeline.
#[async_trait]
pub trait BuilderSemanticCompiler: Send + Sync {
    /// Validate and compile a semantic query (given as raw JSON params) to SQL.
    ///
    /// The params should contain `topic`, `measures`, `dimensions`,
    /// `time_dimensions`, `filters`, `orders`, `limit`, `offset` fields.
    async fn compile(
        &self,
        params: &serde_json::Value,
    ) -> Result<SemanticCompilationResult, ToolError>;

    /// Execute a pre-aggregation SQL against the rollup named by `handle` via
    /// in-process DuckDB. Returns up to `sample_limit` rows plus the true
    /// total row count.
    async fn execute_preagg(
        &self,
        preagg_sql: &str,
        handle: &PreaggHandle,
        sample_limit: u64,
    ) -> Result<QueryResult, ToolError>;
}
