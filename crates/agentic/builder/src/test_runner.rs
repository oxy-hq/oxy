//! Trait for running Oxy test files (.test.yml) from within the builder copilot.
//!
//! Defined here to avoid a circular dependency: `agentic-builder` sits below
//! `agentic-http` in the crate graph, but the eval pipeline lives in `oxy-app`
//! which depends on `agentic-http`.  The HTTP layer implements this trait and
//! injects it into [`crate::solver::BuilderSolver`] at startup.

use std::path::Path;

/// Run Oxy test files (`.test.yml`) and return a JSON summary of the results.
#[async_trait::async_trait]
pub trait BuilderTestRunner: Send + Sync {
    async fn run_tests(
        &self,
        workspace_root: &Path,
        test_file: &str,
    ) -> Result<serde_json::Value, String>;
}
