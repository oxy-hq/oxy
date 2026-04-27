//! Background startup recovery for in-flight agentic runs.
//!
//! [`cleanup_stale_runs`] (called from [`super::entry::new_agentic_state`])
//! only transitions interrupted runs to `task_status = "needs_resume"`. The
//! runs themselves are not re-driven until [`recover_active_runs`] rebuilds a
//! `PlatformContext` + `BuilderBridges` per workspace, and hands them to the
//! pipeline's recovery entry point.
//!
//! Spawned in the background so the HTTP listener can start binding while
//! recovery is still in progress — a stuck workspace should not keep the
//! server from serving healthchecks.

use std::sync::Arc;

use agentic_http::AgenticState;
use agentic_pipeline::BuilderTestRunnerTrait;
use agentic_pipeline::platform::{BuilderBridges, PlatformContext};
use agentic_pipeline::recovery::recover_active_runs;
use agentic_runtime::state::RuntimeState;
use oxy::adapters::workspace::builder::WorkspaceBuilder;
use sea_orm::{DatabaseConnection, EntityTrait};

use crate::agentic_wiring::{OxyProjectContext, build_builder_bridges};
use crate::server::serve_mode::{LOCAL_WORKSPACE_ID, ServeMode};

/// Spawn the graceful-shutdown hook. Returns immediately.
///
/// When `agentic_state.shutdown_token` is cancelled (the serve command
/// forwards SIGINT/SIGTERM into it), mark every active run as `"shutdown"`
/// and signal their cancel channels. Runs with `task_status = "shutdown"`
/// are picked up by [`recover_active_runs`] on the next startup, so this
/// is the resumable counterpart to the `"cancelled"` state set by
/// user-initiated cancel.
pub(super) fn spawn_shutdown_hook(agentic_state: Arc<AgenticState>) {
    let token = agentic_state.shutdown_token.clone();
    let runtime = agentic_state.runtime.clone();
    let db = agentic_state.db.clone();
    tokio::spawn(async move {
        token.cancelled().await;
        let count = runtime.shutdown_all(&db).await;
        if count > 0 {
            tracing::info!(
                target: "recovery",
                count,
                "graceful shutdown: marked active runs resumable"
            );
        }
    });
}

/// Spawn startup recovery in the background. Returns immediately.
///
/// The spawned task rebuilds per-workspace `PlatformContext` + `BuilderBridges`
/// and hands them to [`recover_active_runs`] so runs interrupted by a server
/// restart actually resume instead of sitting in `needs_resume` forever.
pub(super) fn spawn_recovery(agentic_state: Arc<AgenticState>, mode: ServeMode) {
    let db = agentic_state.db.clone();
    let runtime = agentic_state.runtime.clone();
    let schema_cache = Some(agentic_state.schema_cache.clone());
    let builder_test_runner: Option<Arc<dyn BuilderTestRunnerTrait>> =
        agentic_state.builder_test_runner.clone();

    tokio::spawn(async move {
        let recovered = run_recovery(&db, runtime, schema_cache, builder_test_runner, mode).await;
        if recovered > 0 {
            tracing::info!(
                target: "recovery",
                recovered,
                mode = mode.label(),
                "startup recovery resumed interrupted runs"
            );
        }
    });
}

/// Pure async recovery entry point — separated from [`spawn_recovery`] so
/// tests can drive it without a background task.
pub(super) async fn run_recovery(
    db: &DatabaseConnection,
    runtime: Arc<RuntimeState>,
    schema_cache: Option<
        Arc<
            std::sync::Mutex<
                std::collections::HashMap<String, agentic_pipeline::AnalyticsSchemaCatalog>,
            >,
        >,
    >,
    builder_test_runner: Option<Arc<dyn BuilderTestRunnerTrait>>,
    mode: ServeMode,
) -> usize {
    match mode {
        ServeMode::Local => recover_local(db, runtime, schema_cache, builder_test_runner).await,
        ServeMode::Cloud => {
            recover_all_workspaces(db, runtime, schema_cache, builder_test_runner).await
        }
    }
}

async fn recover_local(
    db: &DatabaseConnection,
    runtime: Arc<RuntimeState>,
    schema_cache: Option<
        Arc<
            std::sync::Mutex<
                std::collections::HashMap<String, agentic_pipeline::AnalyticsSchemaCatalog>,
            >,
        >,
    >,
    builder_test_runner: Option<Arc<dyn BuilderTestRunnerTrait>>,
) -> usize {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(target: "recovery", error = %e, "local recovery: no cwd, skipping");
            return 0;
        }
    };

    let workspace_manager = match WorkspaceBuilder::new(LOCAL_WORKSPACE_ID)
        .with_workspace_path_and_fallback_config(&cwd)
        .await
    {
        Ok(b) => match b.build().await {
            Ok(wm) => wm,
            Err(e) => {
                tracing::warn!(
                    target: "recovery",
                    cwd = %cwd.display(),
                    error = %e,
                    "local recovery: failed to build WorkspaceManager"
                );
                return 0;
            }
        },
        Err(e) => {
            tracing::warn!(
                target: "recovery",
                cwd = %cwd.display(),
                error = %e,
                "local recovery: failed to resolve workspace path"
            );
            return 0;
        }
    };

    let project_ctx = Arc::new(OxyProjectContext::new(workspace_manager));
    let platform: Arc<dyn PlatformContext> = project_ctx.clone();
    let bridges: Option<BuilderBridges> = Some(build_builder_bridges(project_ctx));

    recover_active_runs(
        db.clone(),
        runtime,
        platform,
        bridges,
        schema_cache,
        builder_test_runner,
    )
    .await
}

async fn recover_all_workspaces(
    db: &DatabaseConnection,
    runtime: Arc<RuntimeState>,
    schema_cache: Option<
        Arc<
            std::sync::Mutex<
                std::collections::HashMap<String, agentic_pipeline::AnalyticsSchemaCatalog>,
            >,
        >,
    >,
    builder_test_runner: Option<Arc<dyn BuilderTestRunnerTrait>>,
) -> usize {
    let workspaces = match entity::workspaces::Entity::find().all(db).await {
        Ok(ws) => ws,
        Err(e) => {
            tracing::error!(target: "recovery", error = %e, "cloud recovery: failed to list workspaces");
            return 0;
        }
    };

    if workspaces.is_empty() {
        tracing::info!(target: "recovery", "cloud recovery: no workspaces registered");
        return 0;
    }

    let mut total = 0usize;
    for ws in &workspaces {
        let Some(ref path) = ws.path else {
            continue;
        };

        let workspace_manager = match WorkspaceBuilder::new(ws.id)
            .with_workspace_path_and_fallback_config(path)
            .await
        {
            Ok(b) => match b.build().await {
                Ok(wm) => wm,
                Err(e) => {
                    tracing::warn!(
                        target: "recovery",
                        workspace_id = %ws.id,
                        error = %e,
                        "cloud recovery: failed to build WorkspaceManager, skipping"
                    );
                    continue;
                }
            },
            Err(e) => {
                tracing::warn!(
                    target: "recovery",
                    workspace_id = %ws.id,
                    error = %e,
                    "cloud recovery: failed to resolve workspace path, skipping"
                );
                continue;
            }
        };

        let project_ctx = Arc::new(OxyProjectContext::new(workspace_manager));
        let platform: Arc<dyn PlatformContext> = project_ctx.clone();
        let bridges: Option<BuilderBridges> = Some(build_builder_bridges(project_ctx));

        let n = recover_active_runs(
            db.clone(),
            runtime.clone(),
            platform,
            bridges,
            schema_cache.clone(),
            builder_test_runner.clone(),
        )
        .await;

        if n > 0 {
            tracing::info!(
                target: "recovery",
                workspace_id = %ws.id,
                recovered = n,
                "cloud recovery: resumed runs for workspace"
            );
        }
        total += n;
    }
    total
}
