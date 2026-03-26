//! Provider abstraction for project backend operations.
//!
//! [`ProjectBackend`] encapsulates all branching logic that was previously
//! scattered across handlers as `if !app_state.cloud { ... } else { ... }`
//! checks.  Handlers call methods on the backend and receive typed results;
//! they no longer need to know which mode they are running in.

use std::path::{Path, PathBuf};

use chrono::Utc;
use oxy::api_types::{
    BranchType, CommitEntry, ProjectBranch, RecentCommitsResponse, RevisionInfoResponse,
};
use oxy_project::LocalGitService;
use oxy_shared::errors::OxyError;
use tracing::info;
use uuid::Uuid;

use crate::server::service::project::ProjectService as CloudProjectService;

// ─── Backend types ────────────────────────────────────────────────────────────

/// Filesystem-based backend (local mode).
#[derive(Clone, Debug)]
pub struct LocalBackend {
    /// Root directory of the main git repository (contains `.git`).
    pub root: PathBuf,
}

/// Selects between local-git and cloud-API behaviour.
#[derive(Clone, Debug)]
pub enum ProjectBackend {
    Local(LocalBackend),
    Cloud,
}

// ─── ProjectBackend implementation ───────────────────────────────────────────

impl ProjectBackend {
    // ── Utility ──────────────────────────────────────────────────────────────

    pub fn is_local(&self) -> bool {
        matches!(self, ProjectBackend::Local(_))
    }

    pub fn is_cloud(&self) -> bool {
        matches!(self, ProjectBackend::Cloud)
    }

    /// Returns `true` when a remote git repository is configured.
    /// Always `false` in cloud mode (remote is managed by the platform).
    pub async fn has_remote(&self, project_root: &Path) -> bool {
        match self {
            ProjectBackend::Local(_) => LocalGitService::has_remote(project_root).await,
            ProjectBackend::Cloud => false,
        }
    }

    /// Returns `true` when `project_root` contains a local git repository.
    /// Always `false` in cloud mode.
    pub fn is_git_enabled(&self, project_root: &Path) -> bool {
        match self {
            ProjectBackend::Local(_) => LocalGitService::is_git_repo(project_root),
            ProjectBackend::Cloud => false,
        }
    }

    // ── VCS operations ───────────────────────────────────────────────────────

    /// Pull the latest changes for `branch` from the remote.
    pub async fn pull(&self, project_id: Uuid, branch: Option<String>) -> Result<String, OxyError> {
        match self {
            ProjectBackend::Local(local) => {
                let default_branch = LocalGitService::get_default_branch(&local.root).await;
                let requested_branch = branch.clone().unwrap_or_else(|| default_branch.clone());

                if requested_branch != default_branch {
                    LocalGitService::validate_branch_name(&requested_branch)?;
                }

                let worktree_root = if requested_branch != default_branch {
                    LocalGitService::get_worktree_path(&local.root, &requested_branch)
                        .unwrap_or_else(|| local.root.clone())
                } else {
                    local.root.clone()
                };

                if !self.has_remote(&local.root).await {
                    return Err(OxyError::RuntimeError(
                        "No remote configured. Set GIT_REPOSITORY_URL to enable pull.".to_string(),
                    ));
                }

                let token = LocalGitService::get_remote_token().await;

                let current_branch = LocalGitService::get_current_branch(&worktree_root)
                    .await
                    .unwrap_or_default();

                if current_branch == requested_branch {
                    LocalGitService::pull_from_remote(
                        &worktree_root,
                        &requested_branch,
                        token.as_deref(),
                    )
                    .await?;
                } else {
                    info!(
                        "project_root is on '{}', fast-forwarding '{}' via fetch",
                        current_branch, requested_branch
                    );
                    LocalGitService::fetch_branch_ref(
                        &local.root,
                        &requested_branch,
                        token.as_deref(),
                    )
                    .await?;
                }

                Ok("Pulled latest changes from remote".to_string())
            }
            ProjectBackend::Cloud => {
                CloudProjectService::pull_changes(project_id, branch)
                    .await
                    .map_err(|e| OxyError::RuntimeError(format!("{e}")))?;
                Ok("Changes pulled successfully".to_string())
            }
        }
    }

    /// Commit (and optionally push) changes for `branch`.
    ///
    /// When `message` is empty the commit step is skipped entirely — only the
    /// push runs.  This is the correct behaviour for the "Push" CTA which is
    /// only shown when there are no uncommitted changes (just committed-but-
    /// unpushed commits), and avoids `git commit -m ""` being rejected by git.
    pub async fn push(
        &self,
        project_id: Uuid,
        branch: Option<String>,
        message: String,
    ) -> Result<String, OxyError> {
        match self {
            ProjectBackend::Local(local) => {
                let default_branch = LocalGitService::get_default_branch(&local.root).await;
                let commit_root = branch
                    .as_deref()
                    .filter(|b| !b.is_empty() && *b != default_branch.as_str())
                    .and_then(|b| LocalGitService::get_worktree_path(&local.root, b))
                    .unwrap_or_else(|| local.root.clone());

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
            ProjectBackend::Cloud => {
                CloudProjectService::push_changes(project_id, branch, message)
                    .await
                    .map_err(|e| OxyError::RuntimeError(format!("{e}")))?;
                Ok("Changes pushed successfully".to_string())
            }
        }
    }

    /// Force-push the current local state of `branch` to remote.
    pub async fn force_push(&self, branch: Option<&str>) -> Result<String, OxyError> {
        match self {
            ProjectBackend::Local(local) => {
                let default_branch = LocalGitService::get_default_branch(&local.root).await;
                let branch = branch.unwrap_or(&default_branch);
                if branch != default_branch {
                    LocalGitService::validate_branch_name(branch)?;
                }
                let root = if branch != default_branch {
                    LocalGitService::get_worktree_path(&local.root, branch)
                        .unwrap_or_else(|| local.root.clone())
                } else {
                    local.root.clone()
                };
                let token = LocalGitService::get_remote_token().await;
                LocalGitService::force_push_to_remote(&root, token.as_deref()).await?;
                Ok("Force push successful".to_string())
            }
            ProjectBackend::Cloud => Err(OxyError::RuntimeError(
                "Force push is only supported in local git mode".to_string(),
            )),
        }
    }

    /// Write manually-merged content to a conflict file and stage it.
    pub async fn resolve_conflict_with_content(
        &self,
        branch: Option<&str>,
        file_path: &str,
        content: &str,
    ) -> Result<(), OxyError> {
        match self {
            ProjectBackend::Local(local) => {
                let default_branch = LocalGitService::get_default_branch(&local.root).await;
                let branch = branch.unwrap_or(&default_branch);
                if branch != default_branch {
                    LocalGitService::validate_branch_name(branch)?;
                }
                let root = if branch != default_branch {
                    LocalGitService::get_worktree_path(&local.root, branch)
                        .unwrap_or_else(|| local.root.clone())
                } else {
                    local.root.clone()
                };
                LocalGitService::write_and_stage_file(&root, file_path, content).await
            }
            ProjectBackend::Cloud => Err(OxyError::RuntimeError(
                "Conflict resolution is only supported in local git mode".to_string(),
            )),
        }
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
        match self {
            ProjectBackend::Local(local) => {
                let default_branch = LocalGitService::get_default_branch(&local.root).await;
                let branch = branch.unwrap_or(&default_branch);
                if branch != default_branch {
                    LocalGitService::validate_branch_name(branch)?;
                }
                let root = if branch != default_branch {
                    LocalGitService::get_worktree_path(&local.root, branch)
                        .unwrap_or_else(|| local.root.clone())
                } else {
                    local.root.clone()
                };
                LocalGitService::resolve_conflict_file(&root, file_path, use_mine).await
            }
            ProjectBackend::Cloud => Err(OxyError::RuntimeError(
                "Conflict resolution is only supported in local git mode".to_string(),
            )),
        }
    }

    /// Restore conflict markers for a previously-resolved file on `branch`.
    pub async fn unresolve_conflict_file(
        &self,
        branch: Option<&str>,
        file_path: &str,
    ) -> Result<(), OxyError> {
        match self {
            ProjectBackend::Local(local) => {
                let default_branch = LocalGitService::get_default_branch(&local.root).await;
                let branch = branch.unwrap_or(&default_branch);
                if branch != default_branch {
                    LocalGitService::validate_branch_name(branch)?;
                }
                let root = if branch != default_branch {
                    LocalGitService::get_worktree_path(&local.root, branch)
                        .unwrap_or_else(|| local.root.clone())
                } else {
                    local.root.clone()
                };
                LocalGitService::unresolve_conflict_file(&root, file_path).await
            }
            ProjectBackend::Cloud => Err(OxyError::RuntimeError(
                "Conflict resolution is only supported in local git mode".to_string(),
            )),
        }
    }

    /// Hard-reset the working tree for `branch` to `commit`.
    pub async fn reset_to_commit(
        &self,
        branch: Option<&str>,
        commit: &str,
    ) -> Result<(), OxyError> {
        match self {
            ProjectBackend::Local(local) => {
                let default_branch = LocalGitService::get_default_branch(&local.root).await;
                let branch = branch.unwrap_or(&default_branch);
                if branch != default_branch {
                    LocalGitService::validate_branch_name(branch)?;
                }
                let root = if branch != default_branch {
                    LocalGitService::get_worktree_path(&local.root, branch)
                        .unwrap_or_else(|| local.root.clone())
                } else {
                    local.root.clone()
                };
                LocalGitService::reset_to_commit(&root, commit).await
            }
            ProjectBackend::Cloud => Err(OxyError::RuntimeError(
                "Reset is only supported in local git mode".to_string(),
            )),
        }
    }

    /// Returns the N most recent commits on `branch`.
    pub async fn get_recent_commits(
        &self,
        branch: Option<&str>,
        n: usize,
    ) -> RecentCommitsResponse {
        let raw = match self {
            ProjectBackend::Local(local) => {
                let default_branch = LocalGitService::get_default_branch(&local.root).await;
                let branch = branch.unwrap_or(&default_branch);
                let root = if branch != default_branch {
                    LocalGitService::get_worktree_path(&local.root, branch)
                        .unwrap_or_else(|| local.root.clone())
                } else {
                    local.root.clone()
                };
                LocalGitService::get_recent_commits(&root, n).await
            }
            ProjectBackend::Cloud => vec![],
        };
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
        match self {
            ProjectBackend::Local(local) => {
                let default_branch = LocalGitService::get_default_branch(&local.root).await;
                let branch = branch.unwrap_or(&default_branch);
                if branch != default_branch {
                    LocalGitService::validate_branch_name(branch)?;
                }
                let root = if branch != default_branch {
                    LocalGitService::get_worktree_path(&local.root, branch)
                        .unwrap_or_else(|| local.root.clone())
                } else {
                    local.root.clone()
                };
                LocalGitService::abort_rebase(&root).await
            }
            ProjectBackend::Cloud => Err(OxyError::RuntimeError(
                "Abort rebase is only supported in local git mode".to_string(),
            )),
        }
    }

    /// Continue an in-progress rebase on `branch`.
    pub async fn continue_rebase(&self, branch: Option<&str>) -> Result<(), OxyError> {
        match self {
            ProjectBackend::Local(local) => {
                let default_branch = LocalGitService::get_default_branch(&local.root).await;
                let branch = branch.unwrap_or(&default_branch);
                if branch != default_branch {
                    LocalGitService::validate_branch_name(branch)?;
                }
                let root = if branch != default_branch {
                    LocalGitService::get_worktree_path(&local.root, branch)
                        .unwrap_or_else(|| local.root.clone())
                } else {
                    local.root.clone()
                };
                LocalGitService::continue_rebase(&root).await
            }
            ProjectBackend::Cloud => Err(OxyError::RuntimeError(
                "Continue rebase is only supported in local git mode".to_string(),
            )),
        }
    }

    /// Return revision/sync info for `branch`.
    pub async fn revision_info(
        &self,
        project_id: Uuid,
        branch: Option<&str>,
    ) -> Result<RevisionInfoResponse, OxyError> {
        match self {
            ProjectBackend::Local(local) => {
                let default_branch = LocalGitService::get_default_branch(&local.root).await;
                let branch_name = branch.unwrap_or(&default_branch);

                if branch_name != default_branch {
                    LocalGitService::validate_branch_name(branch_name)?;
                }

                let serve_root = if branch_name != default_branch {
                    LocalGitService::get_worktree_path(&local.root, branch_name)
                        .unwrap_or_else(|| local.root.clone())
                } else {
                    local.root.clone()
                };

                let (sha, message) =
                    LocalGitService::get_branch_commit(&local.root, branch_name).await;
                let current_commit = if sha.is_empty() {
                    String::new()
                } else {
                    format!("{} - {}", &sha[..sha.len().min(7)], message)
                };

                let (latest_sha, remote_url) = tokio::join!(
                    async {
                        LocalGitService::get_tracking_ref_sha(&local.root, branch_name)
                            .await
                            .unwrap_or_else(|| sha.clone())
                    },
                    LocalGitService::get_remote_url(&local.root)
                );

                let latest_commit = if latest_sha == sha {
                    current_commit.clone()
                } else {
                    let (lsha, lmsg) =
                        LocalGitService::get_commit_by_sha(&local.root, &latest_sha).await;
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
                } else if LocalGitService::is_behind_remote(&local.root, &sha, &latest_sha).await {
                    "behind".to_string()
                } else {
                    // local SHA differs from remote tracking ref and is not behind
                    // → local has commits not yet pushed to the remote
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
            ProjectBackend::Cloud => {
                CloudProjectService::get_revision_info(project_id, branch.map(String::from))
                    .await
                    .map_err(|e| OxyError::RuntimeError(format!("{e}")))
            }
        }
    }

    /// List all branches for the project.
    pub async fn list_branches(&self, project_id: Uuid) -> Result<Vec<ProjectBranch>, OxyError> {
        match self {
            ProjectBackend::Local(local) => {
                let branch_pairs = LocalGitService::list_branches_with_status(&local.root).await;
                let now = Utc::now().to_string();
                let branches = branch_pairs
                    .into_iter()
                    .map(|(name, sync_status)| ProjectBranch {
                        id: Uuid::nil(),
                        name,
                        revision: String::new(),
                        project_id,
                        branch_type: BranchType::Local,
                        sync_status,
                        created_at: now.clone(),
                        updated_at: now.clone(),
                    })
                    .collect();
                Ok(branches)
            }
            ProjectBackend::Cloud => CloudProjectService::get_project_branches(project_id)
                .await
                .map_err(|e| OxyError::RuntimeError(format!("{e}"))),
        }
    }

    /// Switch to (or create) `branch`, returning its metadata.
    pub async fn switch_branch(
        &self,
        project_id: Uuid,
        branch: &str,
    ) -> Result<ProjectBranch, OxyError> {
        match self {
            ProjectBackend::Local(local) => {
                LocalGitService::get_or_create_worktree(&local.root, branch).await?;
                let now = Utc::now().to_string();
                Ok(ProjectBranch {
                    id: Uuid::nil(),
                    project_id,
                    branch_type: BranchType::Local,
                    name: branch.to_string(),
                    revision: String::new(),
                    sync_status: "synced".to_string(),
                    created_at: now.clone(),
                    updated_at: now,
                })
            }
            ProjectBackend::Cloud => {
                let branch_model =
                    CloudProjectService::switch_project_branch(project_id, branch.to_string())
                        .await
                        .map_err(|e| OxyError::RuntimeError(format!("{e}")))?;
                Ok(ProjectBranch {
                    id: branch_model.id,
                    project_id,
                    branch_type: BranchType::Remote,
                    name: branch_model.name,
                    revision: branch_model.revision,
                    sync_status: branch_model.sync_status,
                    created_at: branch_model.created_at.to_string(),
                    updated_at: branch_model.updated_at.to_string(),
                })
            }
        }
    }

    /// Delete `branch` from the local repository.
    /// Returns an error if called in cloud mode or if `branch` is the default.
    pub async fn delete_branch(&self, _project_id: Uuid, branch: &str) -> Result<(), OxyError> {
        match self {
            ProjectBackend::Local(local) => {
                let default_branch = LocalGitService::get_default_branch(&local.root).await;
                if branch == default_branch {
                    return Err(OxyError::RuntimeError(format!(
                        "Cannot delete the default branch '{default_branch}'"
                    )));
                }
                LocalGitService::validate_branch_name(branch)?;
                LocalGitService::delete_branch(&local.root, branch).await
            }
            ProjectBackend::Cloud => Err(OxyError::RuntimeError(
                "Branch deletion is only supported in local git mode".to_string(),
            )),
        }
    }

    // ── Storage operations ───────────────────────────────────────────────────

    /// Remap `file_path` (resolved against the main project root) to the
    /// corresponding path inside the git worktree for `branch`, when applicable.
    ///
    /// Returns `file_path` unchanged in cloud mode, when no branch is specified,
    /// when the branch equals the default branch, or when no worktree exists yet.
    pub async fn resolve_path(
        &self,
        branch: Option<&str>,
        file_path: PathBuf,
        project_root: &Path,
    ) -> PathBuf {
        match self {
            ProjectBackend::Cloud => file_path,
            ProjectBackend::Local(_) => {
                let branch = match branch {
                    Some(b) if !b.is_empty() => b,
                    _ => return file_path,
                };
                if LocalGitService::validate_branch_name(branch).is_err() {
                    return file_path;
                }
                let default_branch = LocalGitService::get_default_branch(project_root).await;
                if branch == default_branch {
                    return file_path;
                }
                match LocalGitService::get_worktree_path(project_root, branch) {
                    Some(wt) => {
                        LocalGitService::remap_path_to_worktree(project_root, &wt, &file_path)
                    }
                    None => file_path,
                }
            }
        }
    }

    /// Return the filesystem root to use for file-tree listing and diff operations.
    ///
    /// In local mode this is the worktree directory when a non-default branch is
    /// requested; otherwise it is `project_root`.  In cloud mode it is always
    /// `project_root`.
    pub async fn worktree_root(&self, branch: Option<&str>, project_root: &Path) -> PathBuf {
        match self {
            ProjectBackend::Cloud => project_root.to_path_buf(),
            ProjectBackend::Local(_) => {
                let default_branch = LocalGitService::get_default_branch(project_root).await;
                branch
                    .filter(|b| !b.is_empty() && *b != default_branch.as_str())
                    .filter(|b| LocalGitService::validate_branch_name(b).is_ok())
                    .and_then(|b| LocalGitService::get_worktree_path(project_root, b))
                    .unwrap_or_else(|| project_root.to_path_buf())
            }
        }
    }
}
