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

use oxy::adapters::secrets::SecretsManager;
use oxy::adapters::workspace::builder::WorkspaceBuilder;
use oxy::adapters::workspace::manager::WorkspaceManager;
use oxy::config::WorkingCopy;
use sea_orm::{DatabaseConnection, EntityTrait};
use tokio::sync::Mutex as TokioMutex;
use uuid::Uuid;

use crate::server::service::secret_manager::SecretManagerService;

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
///
/// The secrets manager is NOT among the trimmings, and this is the one place
/// where the request path's shape has to be copied rather than simplified —
/// see the comment at its construction below.
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
    let builder = WorkspaceBuilder::new(workspace_id)
        .with_working_copy(
            std::path::Path::new(path),
            None,
            oxy::config::OnMissing::Empty,
        )
        .await
        .map_err(|e| format!("preagg: workspace build failed: {e}"))?;

    // DB-first with env fallback, exactly as the request path does it
    // (`workspace_context::try_attach_workspace_manager`), so a workspace's
    // stored warehouse credentials are visible to the cycle.
    //
    // FAILING here rather than warning, which is where this diverges from the
    // request path: `WorkspaceBuilder::build` falls back to
    // `SecretsManager::from_environment()` when none is set, so without this
    // every `{{ secrets.* }}` in a connection resolves to an EMPTY STRING —
    // and a warehouse driver reads empty as absent, not as an error. A
    // ClickHouse rollup then dials airlayer's default `http://localhost:8123`
    // with `database = ''` and reports a connection failure that names a host
    // nobody configured. On the request path a missing secrets manager
    // degrades a live query the caller can retry; here it would silently
    // rebuild every rollup in the workspace against the wrong warehouse.
    let secrets_manager =
        SecretsManager::from_database_with_env_fallback(SecretManagerService::new(workspace_id))
            .map_err(|e| {
                format!("preagg: secrets manager unavailable for workspace {workspace_id}: {e}")
            })?;

    builder
        .with_secrets_manager(secrets_manager)
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
