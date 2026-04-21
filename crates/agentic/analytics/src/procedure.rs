//! Adapter trait for external procedure runners.
//!
//! The analytics FSM generates `procedure.yml` files on disk and holds only
//! their [`PathBuf`].  All parsing and task execution is owned by the
//! external runner — the FSM boundary is this thin trait.
//!
//! # Usage
//!
//! ```rust,ignore
//! struct MyRunner;
//!
//! #[async_trait::async_trait]
//! impl ProcedureRunner for MyRunner {
//!     async fn run(&self, file_path: &Path) -> Result<ProcedureOutput, ProcedureError> {
//!         // parse procedure.yml, execute tasks, return output
//!     }
//!
//!     async fn search(&self, query: &str) -> Vec<ProcedureRef> {
//!         // scan ./procedures/ and return matching refs
//!     }
//! }
//!
//! let solver = AnalyticsSolver::new(...)
//!     .with_procedure_runner(Arc::new(MyRunner));
//! ```

// ---------------------------------------------------------------------------
// I/O types
// ---------------------------------------------------------------------------

/// A reference to a discoverable procedure file.
///
/// Returned by [`ProcedureRunner::search`] so the clarifying LLM can decide
/// whether to reuse an existing procedure or generate a new one.
#[derive(Debug, Clone)]
pub struct ProcedureRef {
    /// Display name (typically the file stem, e.g. `"monthly_revenue"`).
    pub name: String,
    /// Absolute path to the `procedure.yml` file.
    pub path: std::path::PathBuf,
    /// Short human-readable description scraped from the file, if available.
    pub description: String,
}

/// Pre-extracted result for a single procedure step.
///
/// Table steps carry real columns and typed rows; non-table steps carry a
/// single `"result"` column with the text representation.
///
/// Rows are `Vec<Vec<serde_json::Value>>` so that numeric columns produced by
/// `SUM`/`AVG`/etc. queries arrive as JSON numbers rather than strings,
/// enabling correct chart rendering downstream.
#[derive(Debug, Clone)]
pub struct ProcedureStepResult {
    /// Task name from the procedure YAML.
    pub step_name: String,
    /// Column names — single element `["result"]` for non-table steps.
    pub columns: Vec<String>,
    /// Typed row data (already truncated by the runner).
    pub rows: Vec<Vec<serde_json::Value>>,
    /// Whether the table rows were truncated by the runner.
    pub truncated: bool,
    /// Total number of rows (equals `rows.len()` when not truncated).
    pub total_row_count: u64,
}

/// Raw output returned by the external runner after executing a procedure.
#[derive(Debug, Clone)]
pub struct ProcedureOutput {
    /// One entry per top-level task in execution order.
    ///
    /// The runner is responsible for flattening the procedure's
    /// `OutputContainer::Map` into this ordered vec.
    pub steps: Vec<ProcedureStepResult>,
}

/// Error returned by the external runner.
#[derive(Debug)]
pub struct ProcedureError(pub String);

impl std::fmt::Display for ProcedureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "procedure runner error: {}", self.0)
    }
}

impl std::error::Error for ProcedureError {}

// ---------------------------------------------------------------------------
// Adapter trait
// ---------------------------------------------------------------------------

/// Adapter for procedure search and discovery.
///
/// The analytics FSM uses this to discover existing procedures via the
/// `search_procedures` tool.  Procedure *execution* is delegated to the
/// coordinator-worker architecture (not this trait).
#[async_trait::async_trait]
pub trait ProcedureRunner: Send + Sync {
    /// Search for existing procedures matching `query`.
    ///
    /// Used by the `search_procedures` clarifying tool so the LLM can
    /// discover and reuse existing procedures instead of generating new ones.
    /// Return an empty `Vec` when no runner is configured or no matches found.
    async fn search(&self, query: &str) -> Vec<ProcedureRef>;
}
