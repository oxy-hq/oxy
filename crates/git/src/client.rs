use std::path::{Path, PathBuf};

use async_trait::async_trait;
use oxy_shared::errors::OxyError;

use crate::types::FileStatus;

/// Unified git operations surface.
///
/// All handlers and services talk to this trait. The only implementation is
/// [`crate::cli::CliGitClient`], which shells out to the system `git` binary.
///
/// Methods that accept `token: Option<&str>` convert to `Auth::Bearer`
/// internally and inject the credential via `http.extraHeader`.
#[async_trait]
pub trait GitClient: Send + Sync {
    // ─── Clone / init ──────────────────────────────────────────────────

    /// Clone `repo_url` into `workspace_root`, or `git init` if `repo_url`
    /// is `None`.  No-op if a `.git` already exists.
    async fn clone_or_init(
        &self,
        workspace_root: &Path,
        repo_url: Option<&str>,
        branch: &str,
        token: Option<&str>,
    ) -> Result<(), OxyError>;

    // ─── Repository helpers ────────────────────────────────────────────

    fn is_git_repo(&self, workspace_root: &Path) -> bool;

    async fn ensure_initialized(&self, workspace_root: &Path) -> Result<(), OxyError>;

    async fn has_remote(&self, workspace_root: &Path) -> bool;

    async fn get_default_branch(&self, workspace_root: &Path) -> String;

    // ─── Branch ────────────────────────────────────────────────────────

    fn validate_branch_name(&self, branch: &str) -> Result<(), OxyError>;

    async fn get_current_branch(&self, workspace_root: &Path) -> Result<String, OxyError>;

    async fn fetch_branch_ref(
        &self,
        root: &Path,
        branch: &str,
        token: Option<&str>,
    ) -> Result<(), OxyError>;

    async fn list_branches_with_status(&self, workspace_root: &Path) -> Vec<(String, String)>;

    async fn list_all_branches(
        &self,
        workspace_root: &Path,
        token: Option<&str>,
    ) -> Result<Vec<String>, OxyError>;

    async fn checkout_branch(
        &self,
        workspace_root: &Path,
        branch: &str,
        token: Option<&str>,
    ) -> Result<(), OxyError>;

    async fn delete_branch(&self, workspace_root: &Path, branch: &str) -> Result<(), OxyError>;

    // ─── Worktree ──────────────────────────────────────────────────────

    fn get_worktree_path(&self, workspace_root: &Path, branch: &str) -> Option<PathBuf>;

    async fn get_or_create_worktree(
        &self,
        workspace_root: &Path,
        branch: &str,
    ) -> Result<PathBuf, OxyError>;

    // ─── Commit ────────────────────────────────────────────────────────

    async fn commit_changes(&self, root: &Path, message: &str) -> Result<String, OxyError>;

    async fn get_head_commit_relative_date(&self, root: &Path) -> Option<String>;

    async fn get_recent_commits(
        &self,
        root: &Path,
        n: usize,
    ) -> Vec<(String, String, String, String, String)>;

    async fn get_commit_by_sha(&self, root: &Path, sha: &str) -> (String, String);

    async fn get_branch_commit(&self, root: &Path, branch: &str) -> (String, String);

    // ─── Diff ──────────────────────────────────────────────────────────

    async fn diff_numstat_summary(&self, repo_path: &Path) -> Result<Vec<FileStatus>, OxyError>;

    async fn diff_numstat_ahead(&self, root: &Path) -> Result<Vec<FileStatus>, OxyError>;

    async fn file_at_rev(
        &self,
        repo_path: &Path,
        file_path: &str,
        commit: Option<&str>,
    ) -> Result<String, OxyError>;

    // ─── Push / pull / remote ──────────────────────────────────────────

    async fn push_to_remote(&self, root: &Path, token: Option<&str>) -> Result<(), OxyError>;

    async fn force_push_to_remote(&self, root: &Path, token: Option<&str>) -> Result<(), OxyError>;

    async fn pull_from_remote(
        &self,
        worktree_root: &Path,
        branch: &str,
        token: Option<&str>,
    ) -> Result<(), OxyError>;

    async fn is_behind_remote(&self, root: &Path, local_sha: &str, remote_sha: &str) -> bool;

    async fn get_tracking_ref_sha(&self, root: &Path, branch: &str) -> Option<String>;

    async fn get_remote_url(&self, workspace_root: &Path) -> Option<String>;

    // ─── Rebase / merge ────────────────────────────────────────────────

    fn is_in_conflict(&self, root: &Path) -> bool;

    async fn reset_to_commit(&self, root: &Path, commit: &str) -> Result<(), OxyError>;

    async fn abort_rebase(&self, root: &Path) -> Result<(), OxyError>;

    async fn continue_rebase(&self, root: &Path) -> Result<(), OxyError>;

    // ─── Conflict file staging ─────────────────────────────────────────

    async fn write_and_stage_file(
        &self,
        root: &Path,
        file_path: &str,
        content: &str,
    ) -> Result<(), OxyError>;

    async fn resolve_conflict_file(
        &self,
        root: &Path,
        file_path: &str,
        use_mine: bool,
    ) -> Result<(), OxyError>;

    async fn unresolve_conflict_file(&self, root: &Path, file_path: &str) -> Result<(), OxyError>;
}
