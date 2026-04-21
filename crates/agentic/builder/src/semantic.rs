//! Semantic query compilation abstraction for the builder domain.
//!
//! Replaces direct use of `oxy_workflow::semantic_validator_builder` and
//! `oxy_workflow::semantic_builder` with a trait that the pipeline layer
//! can implement.

use agentic_core::tools::ToolError;
use async_trait::async_trait;

/// Result of compiling a semantic query to SQL.
pub struct SemanticCompilationResult {
    /// The compiled SQL query.
    pub sql: String,
    /// The target database name (from the view's datasource annotation).
    pub database_name: String,
}

/// Compiles semantic layer queries to SQL.
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
}
