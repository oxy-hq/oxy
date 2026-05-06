use std::collections::HashMap;
use std::path::Path;

/// Run a data app (`.app.yml`) and return a JSON summary of per-task results.
///
/// Defined here to avoid a circular dependency: `agentic-builder` sits below
/// `agentic-http` in the crate graph, but `AppService` lives in `oxy-app`
/// which depends on `agentic-http`. The HTTP layer implements this trait and
/// injects it into [`crate::solver::BuilderSolver`] at startup.
#[async_trait::async_trait]
pub trait BuilderAppRunner: Send + Sync {
    /// Execute the app at `app_file` (relative to `workspace_root`) with the
    /// given control `params` and return a JSON summary of per-task results.
    ///
    /// Always runs fresh — bypasses the result cache.
    async fn run_app(
        &self,
        workspace_root: &Path,
        app_file: &str,
        params: HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value, String>;
}
