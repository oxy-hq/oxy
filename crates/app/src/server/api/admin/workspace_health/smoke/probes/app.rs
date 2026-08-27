//! Data App probe: run every task of one `.app.yml` and assert none errored.
//!
//! `AppService::run` drives the app's tasks through the inline automation
//! decider — no coordinator queue, no LLM, and it bypasses the result cache, so
//! the probe always exercises the real warehouse queries rather than replaying a
//! cached parameter hash. Task execution is fail-fast: the first failing task
//! ends the run.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use super::ProbeFailure;
use crate::agentic_wiring::project_ctx::OxyProjectContext;
use crate::server::service::app::AppService;

/// Run one app. `variables` are the control values an app's Controls would
/// supply; empty means the app's own defaults, which is what the `apps: true`
/// sweep does.
pub(crate) async fn run(
    ctx: &Arc<OxyProjectContext>,
    app_path: &PathBuf,
    variables: &HashMap<String, serde_json::Value>,
) -> Result<(), ProbeFailure> {
    let mut service = AppService::new(ctx.workspace_manager().clone());
    service
        .run(app_path, variables.clone())
        .await
        .map(|_| ())
        .map_err(|e| ProbeFailure::Broken(format!("app run failed: {e}")))
}
