use std::path::{Path, PathBuf};

use oxy_shared::errors::OxyError;
use tracing::info;

use crate::cli::{branch, config, repo, run};

/// Directory name for git worktrees inside the project root.
pub const WORKTREES_DIR: &str = ".worktrees";

/// Converts a branch name to a safe directory name.
///
/// `/` is encoded as `--` so that distinct branch names always map to
/// distinct directory names (e.g. `user/alice` → `user--alice` cannot
/// collide with the literal branch `user-alice`).
pub(crate) fn branch_to_dir_name(branch: &str) -> String {
    branch.replace('/', "--")
}

/// Returns the worktree path for `branch` if it exists on disk.
pub fn get_worktree_path(workspace_root: &Path, branch: &str) -> Option<PathBuf> {
    if branch.is_empty() {
        return None;
    }
    let dir = branch_to_dir_name(branch);
    let path = workspace_root.join(WORKTREES_DIR).join(&dir);
    if path.exists() { Some(path) } else { None }
}

/// Returns the worktree path for `branch`, creating the worktree (and the
/// branch, if it does not already exist) when necessary.
///
/// The branch is forked from `HEAD` of the main project directory, so the
/// new branch starts with a clean copy of the current project state.
pub async fn get_or_create_worktree(
    workspace_root: &Path,
    branch_name: &str,
) -> Result<PathBuf, OxyError> {
    let default_branch = repo::get_default_branch(workspace_root).await;
    if branch_name.is_empty() || branch_name == default_branch {
        return Ok(workspace_root.to_path_buf());
    }

    branch::validate_branch_name(branch_name)?;

    let dir_name = branch_to_dir_name(branch_name);
    let worktree_path = workspace_root.join(WORKTREES_DIR).join(&dir_name);

    if worktree_path.exists() {
        return Ok(worktree_path);
    }

    tokio::fs::create_dir_all(workspace_root.join(WORKTREES_DIR))
        .await
        .map_err(|e| OxyError::IOError(format!("Failed to create .worktrees dir: {e}")))?;

    config::ensure_user_config().await?;

    let branch_exists = branch::branch_exists(workspace_root, branch_name).await?;

    let result = if branch_exists {
        run::run(
            workspace_root,
            &[
                "worktree",
                "add",
                &worktree_path.to_string_lossy(),
                branch_name,
            ],
        )
        .await
    } else {
        run::run(
            workspace_root,
            &[
                "worktree",
                "add",
                "-b",
                branch_name,
                &worktree_path.to_string_lossy(),
            ],
        )
        .await
    };

    match result {
        Ok(_) => {}
        Err(e) => {
            if worktree_path.exists() {
                info!(
                    "Worktree at {} already exists (concurrent creation), using it",
                    worktree_path.display()
                );
            } else {
                return Err(e);
            }
        }
    }

    info!(
        "Created git worktree '{}' at {}",
        branch_name,
        worktree_path.display()
    );
    Ok(worktree_path)
}

/// Recursively copies `src` to `dst` using `tokio::fs`.
///
/// Used as a fallback when `rename` fails with EXDEV (cross-device mount).
pub(crate) async fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        tokio::fs::create_dir_all(dst).await?;
        let mut entries = tokio::fs::read_dir(src).await?;
        while let Some(entry) = entries.next_entry().await? {
            let child_dst = dst.join(entry.file_name());
            Box::pin(copy_recursive(&entry.path(), &child_dst)).await?;
        }
    } else {
        tokio::fs::copy(src, dst).await?;
    }
    Ok(())
}
