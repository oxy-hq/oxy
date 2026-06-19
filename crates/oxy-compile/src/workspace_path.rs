//! Resolve a workspace's authoritative on-disk path from its UUID.
//!
//! The cloud-mode worker fleet runs Compile tasks as `TaskScope::Global`,
//! which means any worker can claim a task carrying any `workspace_id`
//! — including workspaces whose source isn't cloned on this worker's
//! local disk. A worker can only compile a workspace whose working copy is
//! already on its disk (compiles run on the node that holds the checkout —
//! the IDE/build singleton); per-worker clone-on-demand is NOT planned.
//!
//! The fix for that limitation isn't here; the fix here is to make the
//! limitation **visible**. We resolve from the DB's `workspaces.path`
//! column (the single source of truth) instead of trusting the
//! executor's bound `PlatformContext` — which is wrong for global
//! tasks — and fail loudly when the path isn't on disk.

use std::path::PathBuf;

use sea_orm::{DatabaseConnection, EntityTrait};
use uuid::Uuid;

use crate::errors::CompileError;

/// Look up `workspaces.path` for `workspace_id`. Returns the
/// authoritative path the workspace is registered at — this is the
/// path the HTTP layer set at workspace import time, not the
/// executor's bound `PlatformContext::workspace_path()`.
///
/// Errors:
///   - `WorkspaceNotFound` when the workspace row doesn't exist.
///   - `Internal` when the `path` column is NULL (local mode + nil
///     UUID is the only canonical occurrence; cloud-mode workspaces
///     always have `path` set at registration time).
pub async fn resolve_workspace_path(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> Result<PathBuf, CompileError> {
    let row = entity::workspaces::Entity::find_by_id(workspace_id)
        .one(db)
        .await
        .map_err(CompileError::Database)?
        .ok_or_else(|| {
            CompileError::WorkspaceNotFound(format!(
                "workspace {workspace_id} not registered in DB"
            ))
        })?;

    row.path.map(PathBuf::from).ok_or_else(|| {
        CompileError::Internal(format!(
            "workspace {workspace_id} has no `path` registered. \
                 This typically means the workspace is local-mode \
                 (nil UUID) and was never persisted at clone time — \
                 use `oxy compile` directly instead of dispatching \
                 via the worker fleet."
        ))
    })
}
