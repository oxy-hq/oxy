pub mod builder;
pub mod manager;

use std::path::{Path, PathBuf};

use sea_orm::EntityTrait;
use uuid::Uuid;

use crate::config::resolve_local_workspace_path;
use crate::database::client::establish_connection;
use crate::github::default_git_client;
use crate::state_dir::get_state_dir;
use oxy_git::GitClient;
use oxy_git::cli::repo::find_git_root;
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

    match git.get_worktree_path(&root, branch) {
        Some(worktree) => Ok(worktree_project_path(&root, worktree)),
        None => Ok(root),
    }
}

/// Re-apply the workspace's in-repo subdirectory onto a worktree path.
///
/// A git worktree is a full checkout of the *repository root*. When a workspace
/// lives in a subdirectory of the repo (`workspace.path` is `…/<repo>/sub/dir`),
/// the project — including `config.yml` — lives at `<worktree>/sub/dir`, not at
/// the worktree root. Without this, branch switching on a subdirectory workspace
/// resolves config from `<worktree>/config.yml`, which does not exist and fails
/// with "No such file or directory".
///
/// Returns `worktree` unchanged when the workspace is at the repository root (or
/// no git root can be found), preserving the non-subdirectory behaviour.
fn worktree_project_path(root: &Path, worktree: PathBuf) -> PathBuf {
    let Some(git_root) = find_git_root(root) else {
        return worktree;
    };
    match root.strip_prefix(&git_root) {
        Ok(subdir) if !subdir.as_os_str().is_empty() => worktree.join(subdir),
        _ => worktree,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn worktree_project_path_appends_subdirectory() {
        // Repo root holds `.git`; the workspace lives in `packages/app`.
        let repo = TempDir::new().unwrap();
        fs::create_dir(repo.path().join(".git")).unwrap();
        let workspace_root = repo.path().join("packages").join("app");
        fs::create_dir_all(&workspace_root).unwrap();

        let worktree = workspace_root.join(".worktrees").join("feature");
        let resolved = worktree_project_path(&workspace_root, worktree.clone());

        // Config must resolve under `<worktree>/packages/app`, since a worktree
        // is a full checkout of the repository root.
        assert_eq!(resolved, worktree.join("packages").join("app"));
    }

    #[test]
    fn worktree_project_path_unchanged_at_repo_root() {
        // Workspace is the repo root itself — no subdirectory to re-apply.
        let repo = TempDir::new().unwrap();
        fs::create_dir(repo.path().join(".git")).unwrap();

        let worktree = repo.path().join(".worktrees").join("feature");
        let resolved = worktree_project_path(repo.path(), worktree.clone());

        assert_eq!(resolved, worktree);
    }

    #[test]
    fn worktree_project_path_unchanged_without_git_root() {
        // No `.git` anywhere up the tree — fall back to the worktree as-is.
        let dir = TempDir::new().unwrap();
        let workspace_root = dir.path().join("sub");
        fs::create_dir_all(&workspace_root).unwrap();

        let worktree = workspace_root.join(".worktrees").join("feature");
        let resolved = worktree_project_path(&workspace_root, worktree.clone());

        assert_eq!(resolved, worktree);
    }
}
