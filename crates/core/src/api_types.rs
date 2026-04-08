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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkspaceBranch {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub branch_type: BranchType,
    pub name: String,
    pub revision: String,
    pub sync_status: String,
    pub created_at: String,
    pub updated_at: String,
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
    pub sync_status: String,
    pub last_sync_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CommitEntry {
    pub hash: String,
    pub short_hash: String,
    pub message: String,
    pub author: String,
    pub date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecentCommitsResponse {
    pub commits: Vec<CommitEntry>,
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
