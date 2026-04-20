pub mod builder;
pub mod manager;

use std::path::PathBuf;

use sea_orm::EntityTrait;
use uuid::Uuid;

use crate::config::resolve_local_workspace_path;
use crate::database::client::establish_connection;
use crate::github::default_git_client;
use crate::state_dir::get_state_dir;
use oxy_git::GitClient;
use oxy_shared::errors::OxyError;

/// Canonical on-disk root for a workspace: `<state_dir>/workspaces/<workspace_id>`.
///
/// Single generation point for a workspace's filesystem location. The value is
/// stored in `workspaces.path` at registration time; no other code should
/// synthesize this path.
pub fn workspace_root_path(workspace_id: Uuid) -> PathBuf {
    get_state_dir()
        .join("workspaces")
        .join(workspace_id.to_string())
}

/// Compute the effective workspace path for a given branch.
///
/// Starts from `workspace_row.path` (the root). When `branch` is non-empty,
/// valid, and not the repo's default branch, overlays the matching worktree
/// when it exists on disk. Falls back to the root otherwise.
///
/// The only place the backend turns `(workspace, branch)` into a filesystem
/// path — both `workspace_middleware` and `resolve_workspace_path` funnel
/// through here so branch/worktree semantics stay consistent.
pub async fn effective_workspace_path(
    workspace_row: &entity::workspaces::Model,
    branch: Option<&str>,
) -> Result<PathBuf, OxyError> {
    let root = workspace_row
        .path
        .clone()
        .map(PathBuf::from)
        .ok_or_else(|| {
            OxyError::ConfigurationError(format!(
                "Workspace {} has no path configured",
                workspace_row.id
            ))
        })?;

    let Some(branch) = branch.map(str::trim).filter(|b| !b.is_empty()) else {
        return Ok(root);
    };

    let git = default_git_client();
    git.validate_branch_name(branch)?;

    if branch == git.get_default_branch(&root).await {
        return Ok(root);
    }

    Ok(git.get_worktree_path(&root, branch).unwrap_or(root))
}

/// Resolve the workspace path for a given workspace ID.
pub async fn resolve_workspace_path(workspace_id: Uuid) -> Result<PathBuf, OxyError> {
    if workspace_id.is_nil() {
        return resolve_local_workspace_path().map_err(|e| {
            OxyError::ConfigurationError(format!("Failed to resolve local project path: {}", e))
        });
    }

    let conn = establish_connection().await?;
    let workspace = entity::prelude::Workspaces::find_by_id(workspace_id)
        .one(&conn)
        .await
        .map_err(|e| OxyError::DBError(e.to_string()))?
        .ok_or_else(|| OxyError::DBError(format!("Workspace {} not found", workspace_id)))?;

    effective_workspace_path(&workspace, None).await
}
