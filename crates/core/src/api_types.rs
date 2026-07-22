// Minimal API types needed by extracted crates (to avoid circular dependencies)
// Full API layer is in oxy_cli

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BranchType {
    Remote,
    Local,
}

/// Where a branch lives. Drives the BranchQuickSwitcher badges and tells the
/// backend whether a switch needs to create a local tracking branch first.
///
/// `Both` covers the normal case: the branch exists locally **and** as
/// `origin/<name>`. `LocalOnly` is a never-pushed local branch. `RemoteOnly`
/// is a branch the user knows about because origin advertises it, but no
/// local checkout has been created yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BranchOrigin {
    LocalOnly,
    RemoteOnly,
    Both,
}

impl From<oxy_git::BranchOrigin> for BranchOrigin {
    fn from(origin: oxy_git::BranchOrigin) -> Self {
        match origin {
            oxy_git::BranchOrigin::LocalOnly => Self::LocalOnly,
            oxy_git::BranchOrigin::RemoteOnly => Self::RemoteOnly,
            oxy_git::BranchOrigin::Both => Self::Both,
        }
    }
}

impl From<oxy_git::LocalRefOrigin> for BranchOrigin {
    fn from(origin: oxy_git::LocalRefOrigin) -> Self {
        match origin {
            oxy_git::LocalRefOrigin::LocalOnly => Self::LocalOnly,
            oxy_git::LocalRefOrigin::Both => Self::Both,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkspaceBranch {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub branch_type: BranchType,
    pub name: String,
    pub revision: String,
    /// Where this branch lives — local-only, remote-only, or both. Drives the
    /// UI badges in BranchQuickSwitcher. Defaults to `LocalOnly` for
    /// backward-compatible responses that haven't been updated yet.
    #[serde(default = "default_branch_origin")]
    pub origin: BranchOrigin,
    pub created_at: String,
    pub updated_at: String,
}

fn default_branch_origin() -> BranchOrigin {
    BranchOrigin::LocalOnly
}

/// Kept for internal backward compatibility — all external code should use [`WorkspaceBranch`].
pub type ProjectBranch = WorkspaceBranch;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RevisionInfoResponse {
    pub base_sha: String,
    pub head_sha: String,
    pub current_revision: String,
    pub latest_revision: String,
    pub current_commit: String,
    pub latest_commit: String,
    /// Number of local commits not on origin. Drives the Push button's
    /// enabled state and badge.
    pub ahead_count: u64,
    /// Number of origin commits not local. Drives the Pull button's enabled
    /// state and badge.
    pub behind_count: u64,
    /// Number of working-tree changes (tracked + untracked + conflicted).
    /// Drives the Commit button's visibility and the status pill's count.
    pub uncommitted_count: u64,
    /// True throughout the rebase/merge lifecycle — from the moment
    /// `rebase-merge`/`rebase-apply`/`MERGE_HEAD` appears until `--continue`
    /// or `--abort` clears it. Includes the "all conflicts staged but not yet
    /// continued" window where HEAD is still detached, so the FE keeps the
    /// Resolve UI active and the backend refuses pushes.
    pub is_in_conflict: bool,
    pub last_sync_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    /// Relative path from the git repository root to the workspace directory,
    /// using forward slashes. `None` when the workspace is at the git root.
    /// Used by the frontend to construct correct per-subfolder GitHub URLs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_subfolder: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CommitEntry {
    pub hash: String,
    pub short_hash: String,
    pub message: String,
    pub author: String,
    /// Relative **committer** date ("9 minutes ago"). Committer, not author:
    /// `git log` orders by committer date, and `pull --rebase` preserves author
    /// dates while rewriting position, so an author-dated label reads as a
    /// sorting bug on any rebased history.
    pub date: String,
    /// `Some(false)` marks a commit that exists only in the local working copy
    /// and has never been pushed. `None` when there is no upstream to compare
    /// against. Local-only commits block fast-forward pulls and make restore
    /// refuse, so they are called out rather than left to be inferred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_remote: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecentCommitsResponse {
    pub commits: Vec<CommitEntry>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkspaceResponse {
    pub id: Uuid,
    pub name: String,
    pub role: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub workspace_info: Option<WorkspaceInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkspaceInfo {
    pub id: Uuid,
    pub name: String,
    pub workspace_id: Uuid,
    pub created_at: String,
    pub updated_at: String,
}

/// Kept for internal backward compatibility — all external code should use [`WorkspaceInfo`].
pub type ProjectInfo = WorkspaceInfo;
