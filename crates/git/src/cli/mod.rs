pub mod auth;
pub mod branch;
pub mod clone;
pub mod commit;
pub mod config;
pub mod diff;
pub mod path;
pub mod push_pull;
pub mod rebase;
pub mod repo;
pub mod run;
pub mod staging;
pub mod worktree;

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use oxy_shared::errors::OxyError;

use crate::client::GitClient;
use crate::types::FileStatus;

/// `GitClient` implementation that shells out to the system `git` binary.
#[derive(Debug, Clone, Default)]
pub struct CliGitClient;

impl CliGitClient {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl GitClient for CliGitClient {
    // ─── Clone / init ──────────────────────────────────────────────────

    async fn clone_or_init(
        &self,
        workspace_root: &Path,
        repo_url: Option<&str>,
        branch_name: &str,
        token: Option<&str>,
    ) -> Result<(), OxyError> {
        clone::clone_or_init(workspace_root, repo_url, branch_name, token).await
    }

    // ─── Repository helpers ────────────────────────────────────────────

    fn is_git_repo(&self, workspace_root: &Path) -> bool {
        repo::is_git_repo(workspace_root)
    }

    async fn ensure_initialized(&self, workspace_root: &Path) -> Result<(), OxyError> {
        repo::ensure_initialized(workspace_root).await
    }

    async fn has_remote(&self, workspace_root: &Path) -> bool {
        repo::has_remote(workspace_root).await
    }

    async fn get_default_branch(&self, workspace_root: &Path) -> String {
        repo::get_default_branch(workspace_root).await
    }

    // ─── Branch ────────────────────────────────────────────────────────

    fn validate_branch_name(&self, branch_name: &str) -> Result<(), OxyError> {
        branch::validate_branch_name(branch_name)
    }

    async fn get_current_branch(&self, workspace_root: &Path) -> Result<String, OxyError> {
        branch::get_current_branch(workspace_root).await
    }

    async fn fetch_branch_ref(
        &self,
        root: &Path,
        branch_name: &str,
        token: Option<&str>,
    ) -> Result<(), OxyError> {
        branch::fetch_branch_ref(root, branch_name, token).await
    }

    async fn list_branches_with_status(&self, workspace_root: &Path) -> Vec<(String, String)> {
        branch::list_branches_with_status(workspace_root).await
    }

    async fn list_all_branches(
        &self,
        workspace_root: &Path,
        token: Option<&str>,
    ) -> Result<Vec<String>, OxyError> {
        branch::list_all_branches(workspace_root, token).await
    }

    async fn checkout_branch(
        &self,
        workspace_root: &Path,
        branch_name: &str,
        token: Option<&str>,
    ) -> Result<(), OxyError> {
        branch::checkout_branch(workspace_root, branch_name, token).await
    }

    async fn delete_branch(
        &self,
        workspace_root: &Path,
        branch_name: &str,
    ) -> Result<(), OxyError> {
        branch::delete_branch(workspace_root, branch_name).await
    }

    // ─── Worktree ──────────────────────────────────────────────────────

    fn get_worktree_path(&self, workspace_root: &Path, branch_name: &str) -> Option<PathBuf> {
        worktree::get_worktree_path(workspace_root, branch_name)
    }

    async fn get_or_create_worktree(
        &self,
        workspace_root: &Path,
        branch_name: &str,
    ) -> Result<PathBuf, OxyError> {
        worktree::get_or_create_worktree(workspace_root, branch_name).await
    }

    // ─── Commit ────────────────────────────────────────────────────────

    async fn commit_changes(&self, root: &Path, message: &str) -> Result<String, OxyError> {
        commit::commit_changes(root, message).await
    }

    async fn get_head_commit_relative_date(&self, root: &Path) -> Option<String> {
        commit::get_head_commit_relative_date(root).await
    }

    async fn get_recent_commits(
        &self,
        root: &Path,
        n: usize,
    ) -> Vec<(String, String, String, String, String)> {
        commit::get_recent_commits(root, n).await
    }

    async fn get_commit_by_sha(&self, root: &Path, sha: &str) -> (String, String) {
        commit::get_commit_by_sha(root, sha).await
    }

    async fn get_branch_commit(&self, root: &Path, branch_name: &str) -> (String, String) {
        commit::get_branch_commit(root, branch_name).await
    }

    // ─── Diff ──────────────────────────────────────────────────────────

    async fn diff_numstat_summary(&self, repo_path: &Path) -> Result<Vec<FileStatus>, OxyError> {
        diff::numstat_summary(repo_path).await
    }

    async fn diff_numstat_ahead(&self, root: &Path) -> Result<Vec<FileStatus>, OxyError> {
        diff::numstat_ahead(root).await
    }

    async fn file_at_rev(
        &self,
        repo_path: &Path,
        file_path: &str,
        commit_ref: Option<&str>,
    ) -> Result<String, OxyError> {
        diff::file_at_rev(repo_path, file_path, commit_ref).await
    }

    // ─── Push / pull / remote ──────────────────────────────────────────

    async fn push_to_remote(&self, root: &Path, token: Option<&str>) -> Result<(), OxyError> {
        push_pull::push_to_remote(root, token).await
    }

    async fn force_push_to_remote(&self, root: &Path, token: Option<&str>) -> Result<(), OxyError> {
        push_pull::force_push_to_remote(root, token).await
    }

    async fn pull_from_remote(
        &self,
        worktree_root: &Path,
        branch_name: &str,
        token: Option<&str>,
    ) -> Result<(), OxyError> {
        push_pull::pull_from_remote(worktree_root, branch_name, token).await
    }

    async fn is_behind_remote(&self, root: &Path, local_sha: &str, remote_sha: &str) -> bool {
        push_pull::is_behind_remote(root, local_sha, remote_sha).await
    }

    async fn get_tracking_ref_sha(&self, root: &Path, branch_name: &str) -> Option<String> {
        push_pull::get_tracking_ref_sha(root, branch_name).await
    }

    async fn get_remote_url(&self, workspace_root: &Path) -> Option<String> {
        push_pull::get_remote_url(workspace_root).await
    }

    // ─── Rebase / merge ────────────────────────────────────────────────

    fn is_in_conflict(&self, root: &Path) -> bool {
        rebase::is_in_conflict(root)
    }

    async fn reset_to_commit(&self, root: &Path, commit_ref: &str) -> Result<(), OxyError> {
        rebase::reset_to_commit(root, commit_ref).await
    }

    async fn abort_rebase(&self, root: &Path) -> Result<(), OxyError> {
        rebase::abort_rebase(root).await
    }

    async fn continue_rebase(&self, root: &Path) -> Result<(), OxyError> {
        rebase::continue_rebase(root).await
    }

    // ─── Conflict file staging ─────────────────────────────────────────

    async fn write_and_stage_file(
        &self,
        root: &Path,
        file_path: &str,
        content: &str,
    ) -> Result<(), OxyError> {
        staging::write_and_stage_file(root, file_path, content).await
    }

    async fn resolve_conflict_file(
        &self,
        root: &Path,
        file_path: &str,
        use_mine: bool,
    ) -> Result<(), OxyError> {
        staging::resolve_conflict_file(root, file_path, use_mine).await
    }

    async fn unresolve_conflict_file(&self, root: &Path, file_path: &str) -> Result<(), OxyError> {
        staging::unresolve_conflict_file(root, file_path).await
    }
}
