use std::path::{Path, PathBuf};

use oxy_shared::errors::OxyError;
use tracing::info;

use crate::cli::{branch, config, repo, run};

/// Directory name for git worktrees inside the project root.
pub const WORKTREES_DIR: &str = ".worktrees";

/// Converts a branch name to a safe directory name by encoding `/` as `--`.
/// Bijective only because `branch::validate_branch_name` rejects names
/// containing `--`.
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

/// A worktree as reported by `git worktree list --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    /// Absolute path to the worktree's working directory.
    pub path: PathBuf,
    /// Branch ref checked out there (e.g. `refs/heads/feature`), or `None` for
    /// a detached HEAD.
    pub branch: Option<String>,
    /// True for the repo's main working tree (always git's first record). The
    /// reaper never touches it.
    pub is_main: bool,
}

/// Lists every worktree git knows about for the repo containing
/// `workspace_root`, including the main working tree (flagged `is_main`).
pub async fn list_worktrees(workspace_root: &Path) -> Result<Vec<WorktreeEntry>, OxyError> {
    let out = run::run(workspace_root, &["worktree", "list", "--porcelain"]).await?;
    Ok(parse_worktree_list(&out))
}

/// Parses `git worktree list --porcelain`. Records are blank-line separated;
/// each starts with a `worktree <path>` line, optionally followed by
/// `branch <ref>` (absent for detached/bare). Git always lists the main
/// working tree first, so the first parsed entry is `is_main`.
fn parse_worktree_list(porcelain: &str) -> Vec<WorktreeEntry> {
    let mut entries = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut branch: Option<String> = None;
    for line in porcelain.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            if let Some(path) = path.take() {
                entries.push(WorktreeEntry {
                    is_main: entries.is_empty(),
                    path,
                    branch: branch.take(),
                });
            }
            path = Some(PathBuf::from(p));
            branch = None;
        } else if let Some(b) = line.strip_prefix("branch ") {
            branch = Some(b.to_string());
        }
    }
    if let Some(path) = path {
        entries.push(WorktreeEntry {
            is_main: entries.is_empty(),
            path,
            branch,
        });
    }
    entries
}

/// Returns true when the worktree at `worktree_path` has no uncommitted changes
/// (`git status --porcelain` is empty). A clean worktree is safe to discard:
/// every change is committed and lives in the repo's shared object store, so
/// removing the working copy loses nothing — the branch ref and its history
/// survive.
pub async fn is_worktree_clean(worktree_path: &Path) -> Result<bool, OxyError> {
    let out = run::run(worktree_path, &["status", "--porcelain"]).await?;
    Ok(out.trim().is_empty())
}

/// Removes the worktree at `worktree_path` **without** deleting its branch ref.
///
/// Unlike [`branch::delete_branch`] (an explicit user action that also runs
/// `branch -D`), this discards only the on-disk working copy — the branch and
/// its commits remain, and the next access re-materialises the worktree via
/// [`get_or_create_worktree`]. Deliberately no `--force`: if the worktree went
/// dirty since the caller's clean-check, `remove` fails closed rather than
/// silently discarding work. `prune` first clears stale metadata from any
/// out-of-band removals.
pub async fn remove_worktree(workspace_root: &Path, worktree_path: &Path) -> Result<(), OxyError> {
    run::run(workspace_root, &["worktree", "prune"]).await?;
    run::run(
        workspace_root,
        &["worktree", "remove", &worktree_path.to_string_lossy()],
    )
    .await?;
    info!("Reaped idle worktree at {}", worktree_path.display());
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_main_plus_worktrees_and_detached() {
        let porcelain = "\
worktree /state/ws/abc
HEAD 1111111111111111111111111111111111111111
branch refs/heads/main

worktree /state/ws/abc/.worktrees/feature
HEAD 2222222222222222222222222222222222222222
branch refs/heads/feature

worktree /state/ws/abc/.worktrees/wip
HEAD 3333333333333333333333333333333333333333
detached
";
        let entries = parse_worktree_list(porcelain);
        assert_eq!(entries.len(), 3);
        // First record is always the main working tree.
        assert!(entries[0].is_main);
        assert_eq!(entries[0].branch.as_deref(), Some("refs/heads/main"));
        // The .worktrees/* entries are never main.
        assert!(!entries[1].is_main);
        assert_eq!(
            entries[1].path,
            PathBuf::from("/state/ws/abc/.worktrees/feature")
        );
        assert_eq!(entries[1].branch.as_deref(), Some("refs/heads/feature"));
        // Detached HEAD → no branch ref.
        assert!(!entries[2].is_main);
        assert_eq!(entries[2].branch, None);
    }

    #[test]
    fn single_worktree_is_main() {
        let entries =
            parse_worktree_list("worktree /state/ws/abc\nHEAD abc\nbranch refs/heads/main\n");
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_main);
    }

    #[test]
    fn empty_output_yields_no_entries() {
        assert!(parse_worktree_list("").is_empty());
    }
}
