//! Per-workspace context construction for the pre-aggregation cycle.
//!
//! The cycle runs as a `TaskSpec::Custom { kind: "preagg_cycle" }` on the
//! worker fleet — any node, picked fresh per task from a bare `workspace_id`
//! in the payload (see `preagg_executor::PreaggTaskExecutor`), the same shape
//! `HealthEvalTaskExecutor` uses. Unlike health eval, a rebuild also needs a
//! [`WorkspaceManager`] (to resolve the view definitions and the pre-agg
//! config), which is what this module builds.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use oxy::adapters::workspace::builder::WorkspaceBuilder;
use oxy::adapters::workspace::manager::WorkspaceManager;
use oxy::config::WorkingCopy;
use sea_orm::{DatabaseConnection, EntityTrait};
use tokio::sync::Mutex as TokioMutex;
use uuid::Uuid;

/// Build a [`WorkspaceManager`] for `workspace_id` from nothing but a
/// database handle — compiled config first (fleet-safe: works on any node,
/// including one with no working copy), FS fallback second (works only on the
/// node that has this workspace checked out, i.e. the ide singleton).
///
/// Trimmed relative to the request-path resolver
/// (`workspace_context::try_attach_workspace_manager`): no branch parameter
/// (a scheduled or on-demand cycle always targets the default branch — same
/// as the pre-scheduling worker), no worktree bookkeeping, no HTTP-shaped
/// error type. Errors are a plain string: the caller reports task failure
/// through the executor's `TaskOutcome`, not an HTTP status.
pub(super) async fn build_workspace_manager(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> Result<WorkspaceManager<WorkingCopy>, String> {
    let row = entity::workspaces::Entity::find_by_id(workspace_id)
        .one(db)
        .await
        .map_err(|e| format!("workspace lookup failed: {e}"))?
        .ok_or_else(|| format!("workspace {workspace_id} does not exist"))?;
    let path = row
        .path
        .as_deref()
        .ok_or_else(|| format!("workspace {workspace_id} has no path"))?;

    // `with_working_copy` is the one terminal this branch kept: it takes the
    // root, an optional pinned revision, and what to do when `config.yml` is
    // absent. The pre-aggregation cycle always targets the default branch, so
    // the revision hint is `None` — the builder resolves the promoted one.
    //
    // `OnMissing::Empty` rather than a hard error because a workspace that has
    // never been compiled is a real state here, and the rebuild has nothing to
    // do rather than something to fail at.
    WorkspaceBuilder::new(workspace_id)
        .with_working_copy(
            std::path::Path::new(path),
            None,
            oxy::config::OnMissing::Empty,
        )
        .await
        .map_err(|e| format!("preagg: workspace build failed: {e}"))?
        .build()
        .await
        .map_err(|e| format!("preagg: workspace build failed: {e}"))
}

/// Per-workspace manifest write lock, keyed by workspace id.
///
/// `manifest.json` is one file per workspace's local cache directory; every
/// rebuild rewrites it, so any two cycles touching the SAME workspace — a
/// scheduled fire racing an on-demand "Rebuild" click — must serialize. Two
/// DIFFERENT workspaces' cycles must not: they write to different directories
/// and a single global lock would only add queueing with no correctness
/// benefit, on a fleet where many workspaces' cycles can legitimately run at
/// once.
pub(super) fn manifest_write_lock_for(workspace_id: Uuid) -> Arc<TokioMutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<Uuid, Arc<TokioMutex<()>>>>> = OnceLock::new();
    let mut locks = LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("preagg manifest lock registry poisoned");
    Arc::clone(
        locks
            .entry(workspace_id)
            .or_insert_with(|| Arc::new(TokioMutex::new(()))),
    )
}
