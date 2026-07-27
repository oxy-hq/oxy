//! Agent probe: ask an agentic agent a fixed question and assert it answers.
//!
//! This is the widest probe and the only one that spends tokens — it exercises
//! the LLM key, the FSM pipeline, tool dispatch, and SQL generation in one shot.
//! Any non-error answer passes; the probe does not judge correctness, only that
//! the round-trip completes. A suspended run (the agent asked a clarifying
//! question) surfaces as an error string from the pipeline, which is the right
//! outcome: a smoke prompt that can't be answered outright is a broken probe.

use std::path::Path;
use std::sync::Arc;

use agentic_pipeline::platform::PlatformContext;

use super::ProbeFailure;
use crate::agentic_wiring::project_ctx::OxyProjectContext;

pub(crate) async fn ask(
    ctx: &Arc<OxyProjectContext>,
    agent_ref: &str,
    prompt: &str,
) -> Result<(), ProbeFailure> {
    // Resolve through the config manager so the path is right regardless of CWD
    // and of git-subdirectory workspace layouts.
    let config_path = ctx
        .workspace_manager()
        .config_manager
        .resolve_file(agent_ref)
        .await
        .map_err(|e| {
            ProbeFailure::Unavailable(format!("agent config '{agent_ref}' not found: {e}"))
        })?;

    let platform: Arc<dyn PlatformContext> = ctx.clone();
    agentic_pipeline::run_agentic_eval(platform, Path::new(&config_path), prompt.to_string())
        .await
        .map(|_| ())
        .map_err(|e| ProbeFailure::Broken(format!("agent '{agent_ref}' failed: {e}")))
}
