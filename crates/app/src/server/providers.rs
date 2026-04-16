//! Provider abstraction for workspace backend operations.
//!
//! [`WorkspaceBackend`] wraps [`LocalBackend`] and provides all git/file
//! operations needed by the API handlers.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use oxy::api_types::{
    BranchType, CommitEntry, ProjectBranch, RecentCommitsResponse, RevisionInfoResponse,
};
use oxy_project::LocalGitService;
use oxy_shared::errors::OxyError;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

// ─── Backend types ────────────────────────────────────────────────────────────

/// Filesystem-based backend.
#[derive(Clone, Debug)]
pub struct LocalBackend {
    /// Root directory of the main git repository (contains `.git`).
    pub root: PathBuf,
}

/// Workspace backend — always local-filesystem mode.
///
/// In multi-workspace mode the active workspace root can change after construction
/// (when the user switches workspaces via `activate_workspace`).  All git operations
/// read `active_workspace_path` at call time so they always operate on the correct
/// directory.  In single-workspace mode `active_workspace_path` is never set and we
/// fall back to `local.root`.
#[derive(Clone, Debug)]
pub struct WorkspaceBackend {
    pub local: LocalBackend,
    /// Shared active-workspace path updated by `activate_workspace`.
    pub active_workspace_path: Arc<RwLock<Option<PathBuf>>>,
}

impl WorkspaceBackend {
    pub fn new(root: PathBuf) -> Self {
        Self {
            local: LocalBackend { root },
            active_workspace_path: Default::default(),
        }
    }
}

// Keep a `Local(LocalBackend)` constructor used by router.rs
impl WorkspaceBackend {
    pub fn from_local(local: LocalBackend) -> Self {
        Self {
            local,
            active_workspace_path: Default::default(),
        }
    }
}

// ─── WorkspaceBackend implementation ───────────────────────────────────────────

impl WorkspaceBackend {
    // ── Utility ──────────────────────────────────────────────────────────────

    pub fn is_local(&self) -> bool {
        true
    }

    /// Return the currently-active workspace root.
    ///
    /// Reads `active_workspace_path` (updated by `activate_workspace`) so that
    /// multi-workspace mode always operates on the correct directory.  Falls back
    /// to `local.root` when no workspace has been explicitly activated (i.e.
    /// single-workspace mode).
    async fn workspace_root(&self) -> PathBuf {
        self.active_workspace_path
            .read()
            .await
            .clone()
            .unwrap_or_else(|| self.local.root.clone())
    }

    /// Returns `true` when a remote git repository is configured.
    pub async fn has_remote(&self, workspace_root: &Path) -> bool {
        LocalGitService::has_remote(workspace_root).await
    }

    /// Returns `true` when `workspace_root` contains a local git repository.
    pub fn is_git_enabled(&self, workspace_root: &Path) -> bool {
        LocalGitService::is_git_repo(workspace_root)
    }

    // ── VCS operations ───────────────────────────────────────────────────────

    /// Pull the latest changes for `branch` from the remote.
    pub async fn pull(
        &self,
        _workspace_id: Uuid,
        branch: Option<String>,
    ) -> Result<String, OxyError> {
        let root = self.workspace_root().await;
        let default_branch = LocalGitService::get_default_branch(&root).await;
        let requested_branch = branch.clone().unwrap_or_else(|| default_branch.clone());

        if requested_branch != default_branch {
            LocalGitService::validate_branch_name(&requested_branch)?;
        }

        let worktree_root = if requested_branch != default_branch {
            LocalGitService::get_worktree_path(&root, &requested_branch)
                .unwrap_or_else(|| root.clone())
        } else {
            root.clone()
        };

        if !self.has_remote(&root).await {
            return Err(OxyError::RuntimeError(
                "No remote configured. Set GIT_REPOSITORY_URL to enable pull.".to_string(),
            ));
        }

        let token = LocalGitService::get_remote_token().await;

        let current_branch = LocalGitService::get_current_branch(&worktree_root)
            .await
            .unwrap_or_default();

        if current_branch == requested_branch {
            LocalGitService::pull_from_remote(&worktree_root, &requested_branch, token.as_deref())
                .await?;
        } else {
            info!(
                "workspace_root is on '{}', fast-forwarding '{}' via fetch",
                current_branch, requested_branch
            );
            LocalGitService::fetch_branch_ref(&root, &requested_branch, token.as_deref()).await?;
        }

        Ok("Pulled latest changes from remote".to_string())
    }

    /// Commit (and optionally push) changes for `branch`.
    ///
    /// When `message` is empty the commit step is skipped entirely — only the
    /// push runs.  This is the correct behaviour for the "Push" CTA which is
    /// only shown when there are no uncommitted changes (just committed-but-
    /// unpushed commits), and avoids `git commit -m ""` being rejected by git.
    pub async fn push(
        &self,
        _workspace_id: Uuid,
        branch: Option<String>,
        message: String,
    ) -> Result<String, OxyError> {
        let root = self.workspace_root().await;
        let default_branch = LocalGitService::get_default_branch(&root).await;
        let commit_root = branch
            .as_deref()
            .filter(|b| !b.is_empty() && *b != default_branch.as_str())
            .and_then(|b| LocalGitService::get_worktree_path(&root, b))
            .unwrap_or_else(|| root.clone());

        if !message.is_empty() {
            LocalGitService::commit_changes(&commit_root, &message).await?;
        }

        if self.has_remote(&commit_root).await {
            let token = LocalGitService::get_remote_token().await;
            LocalGitService::push_to_remote(&commit_root, token.as_deref()).await?;
            Ok("Changes pushed to remote".to_string())
        } else {
            Ok("Changes committed successfully".to_string())
        }
    }

    /// Force-push the current local state of `branch` to remote.
    pub async fn force_push(&self, branch: Option<&str>) -> Result<String, OxyError> {
        let root = self.workspace_root().await;
        let default_branch = LocalGitService::get_default_branch(&root).await;
        let branch = branch.unwrap_or(&default_branch);
        if branch != default_branch {
            LocalGitService::validate_branch_name(branch)?;
        }
        let branch_root = if branch != default_branch {
            LocalGitService::get_worktree_path(&root, branch).unwrap_or_else(|| root.clone())
        } else {
            root.clone()
        };
        let token = LocalGitService::get_remote_token().await;
        LocalGitService::force_push_to_remote(&branch_root, token.as_deref()).await?;
        Ok("Force push successful".to_string())
    }

    /// Write manually-merged content to a conflict file and stage it.
    pub async fn resolve_conflict_with_content(
        &self,
        branch: Option<&str>,
        file_path: &str,
        content: &str,
    ) -> Result<(), OxyError> {
        let root = self.workspace_root().await;
        let default_branch = LocalGitService::get_default_branch(&root).await;
        let branch = branch.unwrap_or(&default_branch);
        if branch != default_branch {
            LocalGitService::validate_branch_name(branch)?;
        }
        let branch_root = if branch != default_branch {
            LocalGitService::get_worktree_path(&root, branch).unwrap_or_else(|| root.clone())
        } else {
            root.clone()
        };
        LocalGitService::write_and_stage_file(&branch_root, file_path, content).await
    }

    /// Resolve a single conflicted file on `branch` by choosing one side.
    /// `use_mine = true`  → keep your local version (--theirs in rebase terms)
    /// `use_mine = false` → accept the remote version (--ours in rebase terms)
    pub async fn resolve_conflict_file(
        &self,
        branch: Option<&str>,
        file_path: &str,
        use_mine: bool,
    ) -> Result<(), OxyError> {
        let root = self.workspace_root().await;
        let default_branch = LocalGitService::get_default_branch(&root).await;
        let branch = branch.unwrap_or(&default_branch);
        if branch != default_branch {
            LocalGitService::validate_branch_name(branch)?;
        }
        let branch_root = if branch != default_branch {
            LocalGitService::get_worktree_path(&root, branch).unwrap_or_else(|| root.clone())
        } else {
            root.clone()
        };
        LocalGitService::resolve_conflict_file(&branch_root, file_path, use_mine).await
    }

    /// Restore conflict markers for a previously-resolved file on `branch`.
    pub async fn unresolve_conflict_file(
        &self,
        branch: Option<&str>,
        file_path: &str,
    ) -> Result<(), OxyError> {
        let root = self.workspace_root().await;
        let default_branch = LocalGitService::get_default_branch(&root).await;
        let branch = branch.unwrap_or(&default_branch);
        if branch != default_branch {
            LocalGitService::validate_branch_name(branch)?;
        }
        let branch_root = if branch != default_branch {
            LocalGitService::get_worktree_path(&root, branch).unwrap_or_else(|| root.clone())
        } else {
            root.clone()
        };
        LocalGitService::unresolve_conflict_file(&branch_root, file_path).await
    }

    /// Hard-reset the working tree for `branch` to `commit`.
    pub async fn reset_to_commit(
        &self,
        branch: Option<&str>,
        commit: &str,
    ) -> Result<(), OxyError> {
        let root = self.workspace_root().await;
        let default_branch = LocalGitService::get_default_branch(&root).await;
        let branch = branch.unwrap_or(&default_branch);
        if branch != default_branch {
            LocalGitService::validate_branch_name(branch)?;
        }
        let branch_root = if branch != default_branch {
            LocalGitService::get_worktree_path(&root, branch).unwrap_or_else(|| root.clone())
        } else {
            root.clone()
        };
        LocalGitService::reset_to_commit(&branch_root, commit).await
    }

    /// Returns the N most recent commits on `branch`.
    pub async fn get_recent_commits(
        &self,
        branch: Option<&str>,
        n: usize,
    ) -> RecentCommitsResponse {
        let root = self.workspace_root().await;
        let default_branch = LocalGitService::get_default_branch(&root).await;
        let branch = branch.unwrap_or(&default_branch);
        let branch_root = if branch != default_branch {
            LocalGitService::get_worktree_path(&root, branch).unwrap_or_else(|| root.clone())
        } else {
            root.clone()
        };
        let raw = LocalGitService::get_recent_commits(&branch_root, n).await;
        RecentCommitsResponse {
            commits: raw
                .into_iter()
                .map(|(hash, short_hash, message, author, date)| CommitEntry {
                    hash,
                    short_hash,
                    message,
                    author,
                    date,
                })
                .collect(),
        }
    }

    /// Abort an in-progress rebase on `branch`.
    pub async fn abort_rebase(&self, branch: Option<&str>) -> Result<(), OxyError> {
        let root = self.workspace_root().await;
        let default_branch = LocalGitService::get_default_branch(&root).await;
        let branch = branch.unwrap_or(&default_branch);
        if branch != default_branch {
            LocalGitService::validate_branch_name(branch)?;
        }
        let branch_root = if branch != default_branch {
            LocalGitService::get_worktree_path(&root, branch).unwrap_or_else(|| root.clone())
        } else {
            root.clone()
        };
        LocalGitService::abort_rebase(&branch_root).await
    }

    /// Continue an in-progress rebase on `branch`.
    pub async fn continue_rebase(&self, branch: Option<&str>) -> Result<(), OxyError> {
        let root = self.workspace_root().await;
        let default_branch = LocalGitService::get_default_branch(&root).await;
        let branch = branch.unwrap_or(&default_branch);
        if branch != default_branch {
            LocalGitService::validate_branch_name(branch)?;
        }
        let branch_root = if branch != default_branch {
            LocalGitService::get_worktree_path(&root, branch).unwrap_or_else(|| root.clone())
        } else {
            root.clone()
        };
        LocalGitService::continue_rebase(&branch_root).await
    }

    /// Return revision/sync info for `branch`.
    pub async fn revision_info(
        &self,
        _workspace_id: Uuid,
        branch: Option<&str>,
    ) -> Result<RevisionInfoResponse, OxyError> {
        let root = self.workspace_root().await;
        let default_branch = LocalGitService::get_default_branch(&root).await;
        let branch_name = branch.unwrap_or(&default_branch);

        if branch_name != default_branch {
            LocalGitService::validate_branch_name(branch_name)?;
        }

        let serve_root = if branch_name != default_branch {
            LocalGitService::get_worktree_path(&root, branch_name).unwrap_or_else(|| root.clone())
        } else {
            root.clone()
        };

        let (sha, message) = LocalGitService::get_branch_commit(&root, branch_name).await;
        let current_commit = if sha.is_empty() {
            String::new()
        } else {
            format!("{} - {}", &sha[..sha.len().min(7)], message)
        };

        let root_clone = root.clone();
        let (latest_sha, remote_url) = tokio::join!(
            async {
                LocalGitService::get_tracking_ref_sha(&root_clone, branch_name)
                    .await
                    .unwrap_or_else(|| sha.clone())
            },
            LocalGitService::get_remote_url(&root)
        );

        let latest_commit = if latest_sha == sha {
            current_commit.clone()
        } else {
            let (lsha, lmsg) = LocalGitService::get_commit_by_sha(&root_clone, &latest_sha).await;
            if lsha.is_empty() {
                String::new()
            } else {
                format!("{} - {}", &lsha[..lsha.len().min(7)], lmsg)
            }
        };

        let sync_status = if LocalGitService::is_in_conflict(&serve_root) {
            "conflict".to_string()
        } else if sha.is_empty() || latest_sha == sha {
            "synced".to_string()
        } else if LocalGitService::is_behind_remote(&root_clone, &sha, &latest_sha).await {
            "behind".to_string()
        } else {
            "ahead".to_string()
        };

        Ok(RevisionInfoResponse {
            base_sha: sha.clone(),
            head_sha: sha.clone(),
            current_revision: sha.clone(),
            latest_revision: latest_sha,
            current_commit,
            latest_commit,
            sync_status,
            last_sync_time: None,
            remote_url,
        })
    }

    /// List all branches for the project.
    pub async fn list_branches(&self, workspace_id: Uuid) -> Result<Vec<ProjectBranch>, OxyError> {
        let root = self.workspace_root().await;
        let branch_pairs = LocalGitService::list_branches_with_status(&root).await;
        let now = Utc::now().to_string();
        let branches = branch_pairs
            .into_iter()
            .map(|(name, sync_status)| ProjectBranch {
                id: Uuid::nil(),
                name,
                revision: String::new(),
                workspace_id,
                branch_type: BranchType::Local,
                sync_status,
                created_at: now.clone(),
                updated_at: now.clone(),
            })
            .collect();
        Ok(branches)
    }

    /// Switch to (or create) `branch`, returning its metadata.
    ///
    /// `base_branch` overrides the fork point when `branch` does not yet exist.
    /// Pass `None` to fall back to git's default (HEAD of the main worktree).
    pub async fn switch_branch(
        &self,
        workspace_id: Uuid,
        branch: &str,
        base_branch: Option<&str>,
    ) -> Result<ProjectBranch, OxyError> {
        let root = self.workspace_root().await;
        // Lazily initialize a git repo when the workspace was created without one
        // (demo / blank workspaces). This is a no-op when .git already exists.
        LocalGitService::ensure_initialized(&root).await?;
        LocalGitService::get_or_create_worktree(&root, branch, base_branch).await?;
        let now = Utc::now().to_string();
        Ok(ProjectBranch {
            id: Uuid::nil(),
            workspace_id,
            branch_type: BranchType::Local,
            name: branch.to_string(),
            revision: String::new(),
            sync_status: "synced".to_string(),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    /// Delete `branch` from the local repository.
    pub async fn delete_branch(&self, _workspace_id: Uuid, branch: &str) -> Result<(), OxyError> {
        let root = self.workspace_root().await;
        let default_branch = LocalGitService::get_default_branch(&root).await;
        if branch == default_branch {
            return Err(OxyError::RuntimeError(format!(
                "Cannot delete the default branch '{default_branch}'"
            )));
        }
        LocalGitService::validate_branch_name(branch)?;
        LocalGitService::delete_branch(&root, branch).await
    }

    // ── Storage operations ───────────────────────────────────────────────────

    /// Remap `file_path` (resolved against the main project root) to the
    /// corresponding path inside the git worktree for `branch`, when applicable.
    pub async fn resolve_path(
        &self,
        branch: Option<&str>,
        file_path: PathBuf,
        workspace_root: &Path,
    ) -> PathBuf {
        let branch = match branch {
            Some(b) if !b.is_empty() => b,
            _ => return file_path,
        };
        if LocalGitService::validate_branch_name(branch).is_err() {
            return file_path;
        }
        let default_branch = LocalGitService::get_default_branch(workspace_root).await;
        if branch == default_branch {
            return file_path;
        }
        match LocalGitService::get_worktree_path(workspace_root, branch) {
            Some(wt) => LocalGitService::remap_path_to_worktree(workspace_root, &wt, &file_path),
            None => file_path,
        }
    }

    /// Return the filesystem root to use for file-tree listing and diff operations.
    pub async fn worktree_root(&self, branch: Option<&str>, workspace_root: &Path) -> PathBuf {
        let default_branch = LocalGitService::get_default_branch(workspace_root).await;
        branch
            .filter(|b| !b.is_empty() && *b != default_branch.as_str())
            .filter(|b| LocalGitService::validate_branch_name(b).is_ok())
            .and_then(|b| LocalGitService::get_worktree_path(workspace_root, b))
            .unwrap_or_else(|| workspace_root.to_path_buf())
    }
}
