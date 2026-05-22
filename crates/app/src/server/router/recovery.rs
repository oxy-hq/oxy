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
use agentic_pipeline::platform::{BuilderBridges, PlatformContext};
use agentic_pipeline::recovery::{recover_active_runs, recover_stranded_runs};
use agentic_pipeline::{BuilderAppRunnerTrait, BuilderTestRunnerTrait};
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

/// Env flag enabling the in-process global driver loop: after one-shot
/// startup recovery, re-run recovery on an interval so scheduler-seeded
/// (Phase 2) and crash-orphaned `scope_owned = false` runs are driven to
/// completion without a per-request coordinator. Default OFF — when unset
/// the loop never spawns and behavior is byte-identical to before.
pub(super) const INPROC_GLOBAL_WORKER_ENV: &str = "OXY_INPROC_GLOBAL_WORKER";
const INPROC_GLOBAL_WORKER_INTERVAL_ENV: &str = "OXY_INPROC_GLOBAL_WORKER_INTERVAL_SECS";
const DEFAULT_INPROC_GLOBAL_WORKER_INTERVAL_SECS: u64 = 30;

fn inproc_global_worker_enabled() -> bool {
    std::env::var(INPROC_GLOBAL_WORKER_ENV)
        .ok()
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

fn inproc_global_worker_interval() -> std::time::Duration {
    let secs = std::env::var(INPROC_GLOBAL_WORKER_INTERVAL_ENV)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_INPROC_GLOBAL_WORKER_INTERVAL_SECS);
    std::time::Duration::from_secs(secs)
}

/// Spawn startup recovery in the background. Returns immediately.
///
/// The spawned task rebuilds per-workspace `PlatformContext` + `BuilderBridges`
/// and hands them to [`recover_active_runs`] so runs interrupted by a server
/// restart actually resume instead of sitting in `needs_resume` forever.
///
/// When [`INPROC_GLOBAL_WORKER_ENV`] is set, the task does not stop after the
/// one-shot pass: it re-runs recovery on an interval (tied to the shutdown
/// token) so the queue has a durable in-process consumer for
/// `scope_owned = false` work. This is safe to call repeatedly because
/// `get_resumable_root_runs` is driver-lease-gated (F1) — an in-flight run a
/// live driver owns is excluded from re-selection.
pub(super) fn spawn_recovery(agentic_state: Arc<AgenticState>, mode: ServeMode) {
    let db = agentic_state.db.clone();
    let runtime = agentic_state.runtime.clone();
    let schema_cache = Some(agentic_state.schema_cache.clone());
    let builder_test_runner: Option<Arc<dyn BuilderTestRunnerTrait>> =
        agentic_state.builder_test_runner.clone();
    let builder_app_runner: Option<Arc<dyn BuilderAppRunnerTrait>> =
        agentic_state.builder_app_runner.clone();
    let router = agentic_state.router.clone();
    let shutdown = agentic_state.shutdown_token.clone();
    // §12 FU4c: long-lived cache so repeated periodic ticks (and the
    // latency worker, below) reuse the per-workspace OxyProjectContext
    // instead of rebuilding WorkspaceManager every cycle.
    let ws_cache = super::workspace_cache::new_workspace_context_cache();

    // §12 FU4c latency worker — shared across local and cloud modes.
    // Drains `queued scope_owned=false` rows at sub-second latency
    // instead of waiting for the periodic loop's grace window.
    //
    // The cloud-mode branch (added when `agentic_runs.workspace_id`
    // landed) groups pending rows by workspace and routes each to the
    // right cached `PlatformContext` via `ws_cache`; no need for one
    // worker per workspace.
    if inproc_global_worker_enabled() {
        spawn_latency_worker(
            db.clone(),
            runtime.clone(),
            schema_cache.clone(),
            builder_test_runner.clone(),
            builder_app_runner.clone(),
            router.clone(),
            mode,
            shutdown.clone(),
            ws_cache.clone(),
        );
    }

    tokio::spawn(async move {
        let recovered = run_recovery(
            &db,
            runtime.clone(),
            schema_cache.clone(),
            builder_test_runner.clone(),
            builder_app_runner.clone(),
            router.clone(),
            mode,
            false, // one-shot startup recovery: every coordinator is dead
            ws_cache.clone(),
        )
        .await;
        if recovered > 0 {
            tracing::info!(
                target: "recovery",
                recovered,
                mode = mode.label(),
                "startup recovery resumed interrupted runs"
            );
        }

        if !inproc_global_worker_enabled() {
            return;
        }
        let interval = inproc_global_worker_interval();
        tracing::info!(
            target: "recovery",
            interval_secs = interval.as_secs(),
            mode = mode.label(),
            "in-process global driver loop enabled (OXY_INPROC_GLOBAL_WORKER)"
        );
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!(target: "recovery", "global driver loop: shutdown");
                    break;
                }
                _ = tokio::time::sleep(interval) => {
                    let n = run_recovery(
                        &db,
                        runtime.clone(),
                        schema_cache.clone(),
                        builder_test_runner.clone(),
                        builder_app_runner.clone(),
                        router.clone(),
                        mode,
                        true, // periodic tick: stranded runs only (never poach a live interactive run)
                        ws_cache.clone(),
                    )
                    .await;
                    if n > 0 {
                        tracing::info!(
                            target: "recovery",
                            picked_up = n,
                            "global driver loop: drove scope_owned=false runs"
                        );
                    }
                }
            }
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
    builder_app_runner: Option<Arc<dyn BuilderAppRunnerTrait>>,
    router: Arc<dyn agentic_runtime::router::TaskRouter>,
    mode: ServeMode,
    // `true` = periodic global-driver tick (drive stranded runs only);
    // `false` = one-shot startup recovery (drive all resumable runs).
    periodic: bool,
    ws_cache: Arc<super::workspace_cache::WorkspaceContextCache>,
) -> usize {
    match mode {
        ServeMode::Local => {
            recover_local(
                db,
                runtime,
                schema_cache,
                builder_test_runner,
                builder_app_runner,
                router,
                periodic,
                ws_cache,
            )
            .await
        }
        ServeMode::Cloud => {
            recover_all_workspaces(
                db,
                runtime,
                schema_cache,
                builder_test_runner,
                builder_app_runner,
                router,
                periodic,
                ws_cache,
            )
            .await
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
    builder_app_runner: Option<Arc<dyn BuilderAppRunnerTrait>>,
    router: Arc<dyn agentic_runtime::router::TaskRouter>,
    periodic: bool,
    ws_cache: Arc<super::workspace_cache::WorkspaceContextCache>,
) -> usize {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(target: "recovery", error = %e, "local recovery: no cwd, skipping");
            return 0;
        }
    };

    let Some(project_ctx) = ws_cache
        .get_or_build(LOCAL_WORKSPACE_ID, || async {
            build_local_project_ctx(&cwd).await
        })
        .await
    else {
        return 0;
    };
    let platform: Arc<dyn PlatformContext> = project_ctx.clone();
    // Reused by the Phase 2 scheduler tick (airway seeds need a workspace
    // surface; OxyProjectContext is also a WorkflowWorkspaceContext).
    let workspace: Arc<dyn agentic_pipeline::WorkflowWorkspaceContext> = project_ctx.clone();
    let bridges: Option<BuilderBridges> = Some(build_builder_bridges(project_ctx));

    if periodic {
        // Local-mode workspace_id == LOCAL_WORKSPACE_ID (Uuid::nil()).
        // Stamped on every local run by the `start_*_run` paths; we
        // filter the selection by it so the SELECT can't drift across
        // workspaces if a cloud DB is ever pointed at a local server.
        let recovered = recover_stranded_runs(
            db.clone(),
            runtime,
            platform,
            bridges,
            schema_cache,
            builder_test_runner,
            builder_app_runner,
            router,
            Some(LOCAL_WORKSPACE_ID),
        )
        .await;
        // Periodic tick drives the cron scheduler, scoped to this
        // workspace (§12 FU4b). Local mode = the single LOCAL_WORKSPACE_ID.
        let fired =
            agentic_pipeline::scheduler::tick_schedules(db, LOCAL_WORKSPACE_ID, workspace.as_ref())
                .await;
        if fired > 0 {
            tracing::info!(target: "scheduler", fired, workspace_id = %LOCAL_WORKSPACE_ID, "periodic tick fired schedules");
        }
        recovered + fired
    } else {
        recover_active_runs(
            db.clone(),
            runtime,
            platform,
            bridges,
            schema_cache,
            builder_test_runner,
            builder_app_runner,
            router,
            Some(LOCAL_WORKSPACE_ID),
        )
        .await
    }
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
    builder_app_runner: Option<Arc<dyn BuilderAppRunnerTrait>>,
    router: Arc<dyn agentic_runtime::router::TaskRouter>,
    periodic: bool,
    ws_cache: Arc<super::workspace_cache::WorkspaceContextCache>,
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

        let Some(project_ctx) = ws_cache
            .get_or_build(ws.id, || async {
                build_cloud_project_ctx(ws.id, path).await
            })
            .await
        else {
            // build_cloud_project_ctx already logged the failure cause.
            continue;
        };
        let platform: Arc<dyn PlatformContext> = project_ctx.clone();
        // §12 FU4b: keep a workspace handle for the per-workspace
        // scheduler tick below; airway targets need it to resolve the
        // pipeline file on THIS workspace's filesystem.
        let workspace: Arc<dyn agentic_pipeline::WorkflowWorkspaceContext> = project_ctx.clone();
        let bridges: Option<BuilderBridges> = Some(build_builder_bridges(project_ctx));

        let n = if periodic {
            // Filter to THIS workspace's runs only — otherwise the
            // loop would drive every workspace's stranded rows with
            // the wrong PlatformContext on each iteration (silently
            // pre-FU4c, now an enforced invariant).
            let recovered = recover_stranded_runs(
                db.clone(),
                runtime.clone(),
                platform,
                bridges,
                schema_cache.clone(),
                builder_test_runner.clone(),
                builder_app_runner.clone(),
                router.clone(),
                Some(ws.id),
            )
            .await;
            // Per-workspace scheduler tick (§12 FU4b). Each workspace
            // only ticks its own schedules.
            let fired =
                agentic_pipeline::scheduler::tick_schedules(db, ws.id, workspace.as_ref()).await;
            if fired > 0 {
                tracing::info!(
                    target: "scheduler",
                    workspace_id = %ws.id,
                    fired,
                    "cloud tick fired schedules"
                );
            }
            recovered + fired
        } else {
            recover_active_runs(
                db.clone(),
                runtime.clone(),
                platform,
                bridges,
                schema_cache.clone(),
                builder_test_runner.clone(),
                builder_app_runner.clone(),
                router.clone(),
                Some(ws.id),
            )
            .await
        };

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

// ── Per-workspace context builders (used by the cache, §12 FU4c) ───────────

/// §12 FU4c latency-worker spawn (local + cloud).
///
/// Polls `find_pending_global_runs` every ~1s and drives any
/// freshly-seeded `queued scope_owned = false` runs immediately, instead
/// of waiting for the periodic loop's grace window (≈ grace + interval).
/// Concurrency-safe vs. the periodic loop: both call `recover_single_run`
/// which CAS-acquires the driver lease, so the loser of any race skips.
///
/// **Local mode:** rebuild/reuse the single `LOCAL_WORKSPACE_ID` context
/// every tick and drive any pending row.
///
/// **Cloud mode:** discover which workspaces actually have pending rows
/// via `find_pending_global_runs(None)`, then drive each workspace's
/// rows with its own cached context. This is a single shared worker, not
/// one-per-workspace — the routing is per-row via `agentic_runs.workspace_id`.
#[allow(clippy::too_many_arguments)]
fn spawn_latency_worker(
    db: sea_orm::DatabaseConnection,
    runtime: Arc<RuntimeState>,
    schema_cache: Option<
        Arc<
            std::sync::Mutex<
                std::collections::HashMap<String, agentic_pipeline::AnalyticsSchemaCatalog>,
            >,
        >,
    >,
    builder_test_runner: Option<Arc<dyn BuilderTestRunnerTrait>>,
    builder_app_runner: Option<Arc<dyn BuilderAppRunnerTrait>>,
    router: Arc<dyn agentic_runtime::router::TaskRouter>,
    mode: ServeMode,
    shutdown: tokio_util::sync::CancellationToken,
    ws_cache: Arc<super::workspace_cache::WorkspaceContextCache>,
) {
    // Configurable for soak tuning; default 1s.
    let poll = match std::env::var("OXY_LATENCY_WORKER_INTERVAL_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&n| n > 0)
    {
        Some(ms) => std::time::Duration::from_millis(ms),
        None => std::time::Duration::from_millis(1000),
    };
    tracing::info!(
        target: "recovery",
        poll_ms = poll.as_millis() as u64,
        mode = mode.label(),
        "latency worker enabled (OXY_INPROC_GLOBAL_WORKER)"
    );
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!(target: "recovery", "latency worker: shutdown");
                    break;
                }
                _ = tokio::time::sleep(poll) => {
                    let n = match mode {
                        ServeMode::Local => {
                            tick_local(
                                &db,
                                &runtime,
                                schema_cache.as_ref(),
                                builder_test_runner.as_ref(),
                                builder_app_runner.as_ref(),
                                &router,
                                &ws_cache,
                            )
                            .await
                        }
                        ServeMode::Cloud => {
                            tick_cloud(
                                &db,
                                &runtime,
                                schema_cache.as_ref(),
                                builder_test_runner.as_ref(),
                                builder_app_runner.as_ref(),
                                &router,
                                &ws_cache,
                            )
                            .await
                        }
                    };
                    if n > 0 {
                        tracing::info!(
                            target: "recovery",
                            picked_up = n,
                            mode = mode.label(),
                            "latency worker: drove pending Global runs"
                        );
                    }
                }
            }
        }
    });
}

/// Local-mode tick: single cached context, no per-row routing needed.
#[allow(clippy::too_many_arguments)]
async fn tick_local(
    db: &sea_orm::DatabaseConnection,
    runtime: &Arc<RuntimeState>,
    schema_cache: Option<
        &Arc<
            std::sync::Mutex<
                std::collections::HashMap<String, agentic_pipeline::AnalyticsSchemaCatalog>,
            >,
        >,
    >,
    builder_test_runner: Option<&Arc<dyn BuilderTestRunnerTrait>>,
    builder_app_runner: Option<&Arc<dyn BuilderAppRunnerTrait>>,
    router: &Arc<dyn agentic_runtime::router::TaskRouter>,
    ws_cache: &Arc<super::workspace_cache::WorkspaceContextCache>,
) -> usize {
    let Ok(cwd) = std::env::current_dir() else {
        return 0;
    };
    let Some(ctx) = ws_cache
        .get_or_build(LOCAL_WORKSPACE_ID, || async {
            build_local_project_ctx(&cwd).await
        })
        .await
    else {
        return 0;
    };
    drive_pending(
        db,
        runtime,
        ctx,
        schema_cache,
        builder_test_runner,
        builder_app_runner,
        router,
        Some(LOCAL_WORKSPACE_ID),
    )
    .await
}

/// Cloud-mode tick: discover which workspaces have pending rows, then
/// drive each one with its own cached context. We probe the unfiltered
/// SELECT once per tick to avoid scanning every workspace in the DB when
/// most have no work; the per-workspace re-select inside `drive_pending`
/// CAS-protects against double-drive across replicas.
#[allow(clippy::too_many_arguments)]
async fn tick_cloud(
    db: &sea_orm::DatabaseConnection,
    runtime: &Arc<RuntimeState>,
    schema_cache: Option<
        &Arc<
            std::sync::Mutex<
                std::collections::HashMap<String, agentic_pipeline::AnalyticsSchemaCatalog>,
            >,
        >,
    >,
    builder_test_runner: Option<&Arc<dyn BuilderTestRunnerTrait>>,
    builder_app_runner: Option<&Arc<dyn BuilderAppRunnerTrait>>,
    router: &Arc<dyn agentic_runtime::router::TaskRouter>,
    ws_cache: &Arc<super::workspace_cache::WorkspaceContextCache>,
) -> usize {
    use std::collections::HashSet;
    let pending = match agentic_runtime::crud::find_pending_global_runs(db, None).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(target: "recovery", error = %e, "latency worker: cloud probe failed");
            return 0;
        }
    };
    if pending.is_empty() {
        return 0;
    }
    let workspaces: HashSet<uuid::Uuid> = pending.iter().map(|r| r.workspace_id).collect();

    let mut total = 0usize;
    for ws_id in workspaces {
        // Resolve workspace path from the workspaces table — cached
        // contexts are keyed by workspace_id, but the FIRST build per id
        // needs the path. Cheap one-row lookup; skipped on cache hits.
        let path = match entity::workspaces::Entity::find_by_id(ws_id).one(db).await {
            Ok(Some(ws)) => match ws.path {
                Some(p) => p,
                None => {
                    tracing::warn!(
                        target: "recovery",
                        workspace_id = %ws_id,
                        "latency worker: workspace has no path; skipping"
                    );
                    continue;
                }
            },
            Ok(None) => {
                tracing::warn!(
                    target: "recovery",
                    workspace_id = %ws_id,
                    "latency worker: pending run references unknown workspace"
                );
                continue;
            }
            Err(e) => {
                tracing::warn!(target: "recovery", error = %e, "latency worker: workspace lookup failed");
                continue;
            }
        };

        let Some(ctx) = ws_cache
            .get_or_build(ws_id, || async {
                build_cloud_project_ctx(ws_id, &path).await
            })
            .await
        else {
            continue;
        };
        total += drive_pending(
            db,
            runtime,
            ctx,
            schema_cache,
            builder_test_runner,
            builder_app_runner,
            router,
            Some(ws_id),
        )
        .await;
    }
    total
}

/// Shared drive helper: hand the (already resolved) workspace context to
/// `recover_pending_global_runs` with the matching workspace filter.
#[allow(clippy::too_many_arguments)]
async fn drive_pending(
    db: &sea_orm::DatabaseConnection,
    runtime: &Arc<RuntimeState>,
    ctx: Arc<OxyProjectContext>,
    schema_cache: Option<
        &Arc<
            std::sync::Mutex<
                std::collections::HashMap<String, agentic_pipeline::AnalyticsSchemaCatalog>,
            >,
        >,
    >,
    builder_test_runner: Option<&Arc<dyn BuilderTestRunnerTrait>>,
    builder_app_runner: Option<&Arc<dyn BuilderAppRunnerTrait>>,
    router: &Arc<dyn agentic_runtime::router::TaskRouter>,
    workspace_id: Option<uuid::Uuid>,
) -> usize {
    let platform: Arc<dyn PlatformContext> = ctx.clone();
    let bridges: Option<BuilderBridges> = Some(build_builder_bridges(ctx));
    agentic_pipeline::recovery::recover_pending_global_runs(
        db.clone(),
        runtime.clone(),
        platform,
        bridges,
        schema_cache.cloned(),
        builder_test_runner.cloned(),
        builder_app_runner.cloned(),
        router.clone(),
        workspace_id,
    )
    .await
}

async fn build_local_project_ctx(cwd: &std::path::Path) -> Option<Arc<OxyProjectContext>> {
    let wm = match WorkspaceBuilder::new(LOCAL_WORKSPACE_ID)
        .with_workspace_path_and_fallback_config(cwd)
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
                return None;
            }
        },
        Err(e) => {
            tracing::warn!(
                target: "recovery",
                cwd = %cwd.display(),
                error = %e,
                "local recovery: failed to resolve workspace path"
            );
            return None;
        }
    };
    Some(Arc::new(OxyProjectContext::new(wm)))
}

async fn build_cloud_project_ctx(
    workspace_id: uuid::Uuid,
    path: &str,
) -> Option<Arc<OxyProjectContext>> {
    let wm = match WorkspaceBuilder::new(workspace_id)
        .with_workspace_path_and_fallback_config(path)
        .await
    {
        Ok(b) => match b.build().await {
            Ok(wm) => wm,
            Err(e) => {
                tracing::warn!(
                    target: "recovery",
                    %workspace_id,
                    error = %e,
                    "cloud recovery: failed to build WorkspaceManager, skipping"
                );
                return None;
            }
        },
        Err(e) => {
            tracing::warn!(
                target: "recovery",
                %workspace_id,
                error = %e,
                "cloud recovery: failed to resolve workspace path, skipping"
            );
            return None;
        }
    };
    Some(Arc::new(OxyProjectContext::new(wm)))
}
