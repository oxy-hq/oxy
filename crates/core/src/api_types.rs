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
pub struct ProjectBranch {
    pub id: Uuid,
    pub project_id: Uuid,
    pub branch_type: BranchType,
    pub name: String,
    pub revision: String,
    pub sync_status: String,
    pub created_at: String,
    pub updated_at: String,
}

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
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkspaceResponse {
    pub id: Uuid,
    pub name: String,
    pub role: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub project: Option<ProjectInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProjectInfo {
    pub id: Uuid,
    pub name: String,
    pub workspace_id: Uuid,
    pub created_at: String,
    pub updated_at: String,
}
