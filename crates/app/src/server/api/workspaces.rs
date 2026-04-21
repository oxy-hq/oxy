use axum::extract::State;
use chrono::{DateTime, Utc};
use entity::settings::SyncStatus;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

use crate::server::api::middlewares::workspace_context::{
    BranchQuery, WorkspaceManagerExtractor, WorkspacePath,
};
use crate::server::router::AppState;
use axum::extract::Extension;
use oxy::adapters::workspace::builder::WorkspaceBuilder;
use oxy::adapters::workspace::effective_workspace_path;
use oxy::api_types::{
    BranchType, CommitEntry, ProjectBranch, RecentCommitsResponse, RevisionInfoResponse,
};
use oxy::config::ConfigBuilder;
use oxy::github::{default_git_client, github_token_for_workspace};
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_git::GitClient;
use oxy_shared::errors::OxyError;

use axum::{
    extract::{Json, Path, Query},
    response::Json as ResponseJson,
};

use utoipa::ToSchema;

/// API wrapper for SyncStatus that implements ToSchema
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApiSyncStatus {
    Idle,
    Syncing,
    Synced,
    Error,
}

impl From<SyncStatus> for ApiSyncStatus {
    fn from(status: SyncStatus) -> Self {
        match status {
            SyncStatus::Idle => ApiSyncStatus::Idle,
            SyncStatus::Syncing => ApiSyncStatus::Syncing,
            SyncStatus::Synced => ApiSyncStatus::Synced,
            SyncStatus::Error => ApiSyncStatus::Error,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SwitchBranchRequest {
    pub branch: String,
    /// Optional fork point when creating a new branch.  Ignored when `branch`
    /// already exists.  Defaults to git's `HEAD` of the main worktree.
    #[serde(default)]
    pub base_branch: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProjectResponse {
    pub success: bool,
    pub message: String,
}

/// The single source of truth for the workspace's git state. Only three
/// shapes are valid; representing them as one enum (rather than two booleans)
/// makes the impossible state `(no .git, but has remote)` unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GitMode {
    /// No `.git` directory on disk. Pure local mode — no git UI.
    None,
    /// `.git` exists but no remote configured. Commits are local-only.
    Local,
    /// `.git` exists and a remote is configured (or `GIT_REPOSITORY_URL` is set).
    Connected,
}

/// What the workspace's git mode allows. Derived from `GitMode` via
/// `GitCapabilities::from(mode)` — never set ad-hoc. Adding a new git
/// operation = one row here, no scattered conditionals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GitCapabilities {
    pub can_commit: bool,
    pub can_browse_history: bool,
    pub can_reset_to_commit: bool,
    pub can_switch_branch: bool,
    pub can_diff: bool,
    pub can_push: bool,
    pub can_pull: bool,
    pub can_fetch: bool,
    pub can_force_push: bool,
    pub can_rebase: bool,
    pub can_open_pr: bool,
    pub auto_feature_branch_on_protected: bool,
}

impl From<GitMode> for GitCapabilities {
    fn from(mode: GitMode) -> Self {
        let local = matches!(mode, GitMode::Local | GitMode::Connected);
        let connected = matches!(mode, GitMode::Connected);
        Self {
            can_commit: local,
            can_browse_history: local,
            can_reset_to_commit: local,
            can_switch_branch: local,
            can_diff: local,
            can_push: connected,
            can_pull: connected,
            can_fetch: connected,
            can_force_push: connected,
            can_rebase: connected,
            can_open_pr: connected,
            auto_feature_branch_on_protected: connected,
        }
    }
}

/// Detect the workspace's git mode from disk + environment.
///
/// `GIT_REPOSITORY_URL` is treated as "remote configured" even when the local
/// repo has no remote of its own — that env var is how Oxy injects a remote
/// for cloud-style deployments.
pub async fn detect_git_mode(workspace_root: &std::path::Path) -> GitMode {
    let git = default_git_client();
    if !git.is_git_repo(workspace_root) {
        return GitMode::None;
    }
    let has_remote =
        std::env::var("GIT_REPOSITORY_URL").is_ok() || git.has_remote(workspace_root).await;
    if has_remote {
        GitMode::Connected
    } else {
        GitMode::Local
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkspaceDetailsResponse {
    pub id: Uuid,
    pub name: String,
    pub workspace_id: Uuid,
    pub active_branch: Option<ProjectBranch>,
    pub created_at: String,
    pub updated_at: String,

    /// True when this workspace is registered but its directory does not exist
    /// on disk (e.g. deleted externally). Frontend should show a toast.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_error: Option<String>,

    /// Single source of truth for the workspace's git state.
    pub git_mode: GitMode,

    /// What the current `git_mode` allows. Derived from `git_mode`; the
    /// frontend should branch on these flags rather than on `git_mode`
    /// directly so that adding a new operation only requires one change.
    pub capabilities: GitCapabilities,

    /// Default branch (e.g. "main"). Only meaningful when `git_mode != None`.
    pub default_branch: String,

    /// Branches where saving a file auto-creates a feature branch. Configured
    /// via `protected_branches` in config.yml; defaults to `[default_branch]`.
    pub protected_branches: Vec<String>,

    /// True when this workspace is in local mode and no `config.yml` is
    /// resolvable. The frontend should render the setup dialog instead of
    /// the main app. Always `false` in cloud mode.
    #[serde(default)]
    pub requires_local_setup: bool,
}

// BranchType and ProjectBranch imported from oxy::api_types

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkspaceBranchesResponse {
    pub branches: Vec<ProjectBranch>,
}

async fn workspace_root(ws: &entity::workspaces::Model) -> Result<std::path::PathBuf, StatusCode> {
    effective_workspace_path(ws, None).await.map_err(|e| {
        error!("Failed to resolve workspace path for {}: {}", ws.id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

async fn git_pull(
    worktree: &std::path::Path,
    branch: &str,
    workspace: &entity::workspaces::Model,
) -> Result<String, OxyError> {
    let git = default_git_client();
    if !git.has_remote(worktree).await {
        return Err(OxyError::RuntimeError(
            "No remote configured. Set GIT_REPOSITORY_URL to enable pull.".to_string(),
        ));
    }
    let token = github_token_for_workspace(workspace).await?;
    let current_branch = git.get_current_branch(worktree).await.unwrap_or_default();
    if current_branch == branch {
        git.pull_from_remote(worktree, branch, token.as_deref())
            .await?;
    } else {
        info!(
            "worktree is on '{}', fast-forwarding '{}' via fetch",
            current_branch, branch
        );
        git.fetch_branch_ref(worktree, branch, token.as_deref())
            .await?;
    }
    Ok("Pulled latest changes from remote".to_string())
}

async fn git_push(
    worktree: &std::path::Path,
    message: &str,
    workspace: &entity::workspaces::Model,
) -> Result<String, OxyError> {
    let git = default_git_client();
    if !message.is_empty() {
        git.commit_changes(worktree, message).await?;
    }
    if git.has_remote(worktree).await {
        let token = github_token_for_workspace(workspace).await?;
        git.push_to_remote(worktree, token.as_deref()).await?;
        Ok("Changes pushed to remote".to_string())
    } else {
        Ok("Changes committed successfully".to_string())
    }
}

async fn git_force_push(
    worktree: &std::path::Path,
    workspace: &entity::workspaces::Model,
) -> Result<String, OxyError> {
    let git = default_git_client();
    let token = github_token_for_workspace(workspace).await?;
    git.force_push_to_remote(worktree, token.as_deref()).await?;
    Ok("Force push successful".to_string())
}

async fn git_revision_info(worktree: &std::path::Path, branch: &str) -> RevisionInfoResponse {
    let git = default_git_client();
    let (sha, message) = git.get_branch_commit(worktree, branch).await;
    let current_commit = if sha.is_empty() {
        String::new()
    } else {
        format!("{} - {}", &sha[..sha.len().min(7)], message)
    };

    let (latest_sha, remote_url) = tokio::join!(
        async {
            git.get_tracking_ref_sha(worktree, branch)
                .await
                .unwrap_or_else(|| sha.clone())
        },
        git.get_remote_url(worktree)
    );

    let latest_commit = if latest_sha == sha {
        current_commit.clone()
    } else {
        let (lsha, lmsg) = git.get_commit_by_sha(worktree, &latest_sha).await;
        if lsha.is_empty() {
            String::new()
        } else {
            format!("{} - {}", &lsha[..lsha.len().min(7)], lmsg)
        }
    };

    let sync_status = if git.is_in_conflict(worktree) {
        "conflict".to_string()
    } else if sha.is_empty() || latest_sha == sha {
        "synced".to_string()
    } else if git.is_behind_remote(worktree, &sha, &latest_sha).await {
        "behind".to_string()
    } else {
        "ahead".to_string()
    };

    RevisionInfoResponse {
        base_sha: sha.clone(),
        head_sha: sha.clone(),
        current_revision: sha.clone(),
        latest_revision: latest_sha,
        current_commit,
        latest_commit,
        sync_status,
        last_sync_time: None,
        remote_url,
    }
}

async fn git_list_branches(root: &std::path::Path, workspace_id: Uuid) -> Vec<ProjectBranch> {
    let git = default_git_client();
    let branch_pairs = git.list_branches_with_status(root).await;
    let now = Utc::now().to_string();
    branch_pairs
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
        .collect()
}

async fn git_switch_branch(
    root: &std::path::Path,
    branch: &str,
    workspace_id: Uuid,
) -> Result<ProjectBranch, OxyError> {
    let git = default_git_client();
    git.ensure_initialized(root).await?;
    git.get_or_create_worktree(root, branch).await?;
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

async fn git_delete_branch(root: &std::path::Path, branch: &str) -> Result<(), OxyError> {
    let git = default_git_client();
    let default_branch = git.get_default_branch(root).await;
    if branch == default_branch {
        return Err(OxyError::RuntimeError(format!(
            "Cannot delete the default branch '{default_branch}'"
        )));
    }
    git.validate_branch_name(branch)?;
    git.delete_branch(root, branch).await
}

async fn git_recent_commits(worktree: &std::path::Path, n: usize) -> RecentCommitsResponse {
    let git = default_git_client();
    let raw = git.get_recent_commits(worktree, n).await;
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

// ─── Handlers ────────────────────────────────────────────────────────────

/// Resolve the branch name from `?branch=`, falling back to the repo's default.
async fn resolve_branch(query_branch: Option<String>, root: &std::path::Path) -> String {
    match query_branch.filter(|b| !b.is_empty()) {
        Some(b) => b,
        None => default_git_client().get_default_branch(root).await,
    }
}

pub async fn pull_changes(
    Extension(ws): Extension<entity::workspaces::Model>,
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
    Query(query): Query<BranchQuery>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    let worktree = wm.config_manager.workspace_path();
    let branch = resolve_branch(query.branch, worktree).await;

    match git_pull(worktree, &branch, &ws).await {
        Ok(message) => Ok(ResponseJson(ProjectResponse {
            success: true,
            message,
        })),
        Err(e) => {
            error!("Failed to pull changes: {}", e);
            Ok(ResponseJson(ProjectResponse {
                success: false,
                message: format!("{e}"),
            }))
        }
    }
}

#[derive(Deserialize)]
pub struct ResolveConflictQuery {
    pub branch: Option<String>,
    pub file: String,
    /// `"mine"` = keep your local version; `"theirs"` = accept the remote version
    pub side: String,
}

#[derive(Deserialize)]
pub struct ResolveConflictWithContentQuery {
    pub branch: Option<String>,
    pub file: String,
}

#[derive(Deserialize)]
pub struct UnresolveConflictQuery {
    pub branch: Option<String>,
    pub file: String,
}

#[derive(Deserialize)]
pub struct ResetToCommitQuery {
    pub branch: Option<String>,
    pub commit: String,
}

#[derive(Deserialize)]
pub struct ResolveConflictWithContentBody {
    pub content: String,
}

pub async fn resolve_conflict_with_content(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
    Query(query): Query<ResolveConflictWithContentQuery>,
    Json(body): Json<ResolveConflictWithContentBody>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    let worktree = wm.config_manager.workspace_path();
    match default_git_client()
        .write_and_stage_file(worktree, &query.file, &body.content)
        .await
    {
        Ok(()) => Ok(ResponseJson(ProjectResponse {
            success: true,
            message: format!("Resolved {}", query.file),
        })),
        Err(e) => {
            error!("Failed to resolve conflict with content: {}", e);
            Ok(ResponseJson(ProjectResponse {
                success: false,
                message: format!("{e}"),
            }))
        }
    }
}

pub async fn resolve_conflict_file(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
    Query(query): Query<ResolveConflictQuery>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    let use_mine = query.side == "mine";
    let worktree = wm.config_manager.workspace_path();
    match default_git_client()
        .resolve_conflict_file(worktree, &query.file, use_mine)
        .await
    {
        Ok(()) => Ok(ResponseJson(ProjectResponse {
            success: true,
            message: format!("Resolved {} using {}", query.file, query.side),
        })),
        Err(e) => {
            error!("Failed to resolve conflict file: {}", e);
            Ok(ResponseJson(ProjectResponse {
                success: false,
                message: format!("{e}"),
            }))
        }
    }
}

pub async fn unresolve_conflict_file(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
    Query(query): Query<UnresolveConflictQuery>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    let worktree = wm.config_manager.workspace_path();
    match default_git_client()
        .unresolve_conflict_file(worktree, &query.file)
        .await
    {
        Ok(()) => Ok(ResponseJson(ProjectResponse {
            success: true,
            message: format!("Conflict markers restored for {}", query.file),
        })),
        Err(e) => {
            error!("Failed to unresolve conflict file: {}", e);
            Ok(ResponseJson(ProjectResponse {
                success: false,
                message: format!("{e}"),
            }))
        }
    }
}

pub async fn force_push_branch(
    Extension(ws): Extension<entity::workspaces::Model>,
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    let worktree = wm.config_manager.workspace_path();
    match git_force_push(worktree, &ws).await {
        Ok(message) => Ok(ResponseJson(ProjectResponse {
            success: true,
            message,
        })),
        Err(e) => {
            error!("Failed to force push: {}", e);
            Ok(ResponseJson(ProjectResponse {
                success: false,
                message: format!("{e}"),
            }))
        }
    }
}

pub async fn get_recent_commits(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
) -> Result<ResponseJson<RecentCommitsResponse>, StatusCode> {
    let worktree = wm.config_manager.workspace_path();
    Ok(ResponseJson(git_recent_commits(worktree, 10).await))
}

pub async fn reset_to_commit(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
    Query(query): Query<ResetToCommitQuery>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    let worktree = wm.config_manager.workspace_path();
    match default_git_client()
        .reset_to_commit(worktree, &query.commit)
        .await
    {
        Ok(()) => Ok(ResponseJson(ProjectResponse {
            success: true,
            message: format!("Restored to {}", query.commit),
        })),
        Err(e) => {
            error!("Failed to restore to commit: {}", e);
            Ok(ResponseJson(ProjectResponse {
                success: false,
                message: format!("{e}"),
            }))
        }
    }
}

pub async fn abort_rebase(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    let worktree = wm.config_manager.workspace_path();
    match default_git_client().abort_rebase(worktree).await {
        Ok(()) => Ok(ResponseJson(ProjectResponse {
            success: true,
            message: "Rebase aborted".to_string(),
        })),
        Err(e) => {
            error!("Failed to abort rebase: {}", e);
            Ok(ResponseJson(ProjectResponse {
                success: false,
                message: format!("{e}"),
            }))
        }
    }
}

pub async fn continue_rebase(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    let worktree = wm.config_manager.workspace_path();
    match default_git_client().continue_rebase(worktree).await {
        Ok(()) => Ok(ResponseJson(ProjectResponse {
            success: true,
            message: "Rebase continued successfully".to_string(),
        })),
        Err(e) => {
            error!("Failed to continue rebase: {}", e);
            Ok(ResponseJson(ProjectResponse {
                success: false,
                message: format!("{e}"),
            }))
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PushChangesRequest {
    pub commit_message: Option<String>,
}

pub async fn push_changes(
    Extension(ws): Extension<entity::workspaces::Model>,
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
    Json(request): Json<PushChangesRequest>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    let worktree = wm.config_manager.workspace_path();
    let commit_message = request
        .commit_message
        .unwrap_or_else(|| "Auto-commit: Oxy changes".to_string());

    match git_push(worktree, &commit_message, &ws).await {
        Ok(message) => Ok(ResponseJson(ProjectResponse {
            success: true,
            message,
        })),
        Err(e) => {
            error!("Failed to push changes: {}", e);
            Ok(ResponseJson(ProjectResponse {
                success: false,
                message: format!("{e}"),
            }))
        }
    }
}

// RevisionInfoResponse imported from oxy::api_types

pub async fn get_revision_info(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
    Query(query): Query<BranchQuery>,
) -> Result<ResponseJson<RevisionInfoResponse>, StatusCode> {
    let worktree = wm.config_manager.workspace_path();
    let branch = resolve_branch(query.branch, worktree).await;
    Ok(ResponseJson(git_revision_info(worktree, &branch).await))
}

/// Get detailed information about a specific workspace including its active branch.
///
/// This endpoint retrieves complete workspace details along with the currently active branch information.
/// Requires authentication and returns workspace metadata, provider info, and active branch details.
#[utoipa::path(
    get,
    path = "/workspaces/{workspace_id}",
    params(
        ("workspace_id" = Uuid, Path, description = "Workspace ID")
    ),
    responses(
        (status = 200, description = "Workspace details retrieved successfully", body = WorkspaceDetailsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Workspace not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Workspaces"
)]
pub async fn get_workspace(
    State(app_state): State<AppState>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Extension(project): Extension<entity::workspaces::Model>,
    Path(workspace_id): Path<Uuid>,
) -> Result<ResponseJson<WorkspaceDetailsResponse>, StatusCode> {
    info!("Getting workspace details for ID: {}", workspace_id);

    // Local-mode short-circuit: when there is no resolvable config.yml,
    // skip the normal path (which 500s on path: None) and return the
    // "setup required" shape so the FE renders the bootstrap dialog.
    if app_state.mode.is_local() {
        let config_exists = match &project.path {
            Some(p) => tokio::fs::try_exists(std::path::Path::new(p).join("config.yml"))
                .await
                .unwrap_or(false),
            None => false,
        };
        if !config_exists {
            return Ok(build_workspace_details_response_for_uninitialized_local(
                workspace_id,
                &project.name,
            ));
        }
    }

    let workspace_root = workspace_root(&project).await?;

    build_workspace_details_response(
        workspace_id,
        &project.name,
        &workspace_root,
        app_state.mode.is_local(),
    )
    .await
}

/// Build a `WorkspaceDetailsResponse` in one of the two "no git visible"
/// shapes. Shared between the missing-directory branch and the
/// local-mode short-circuit so a future field addition only lands in
/// one place.
fn no_git_response(
    workspace_id: Uuid,
    name: &str,
    now: String,
    workspace_error: Option<String>,
    requires_local_setup: bool,
) -> ResponseJson<WorkspaceDetailsResponse> {
    let mode = GitMode::None;
    ResponseJson(WorkspaceDetailsResponse {
        id: workspace_id,
        name: name.to_string(),
        workspace_id: Uuid::nil(),
        created_at: now.clone(),
        updated_at: now,
        active_branch: None,
        workspace_error,
        git_mode: mode,
        capabilities: mode.into(),
        default_branch: "main".to_string(),
        protected_branches: vec!["main".to_string()],
        requires_local_setup,
    })
}

/// Response builder for the local-mode "no config.yml yet" case. Exposed
/// publicly so integration tests can assert the shape without spinning up
/// the full router + DB.
pub fn build_workspace_details_response_for_uninitialized_local(
    workspace_id: Uuid,
    name: &str,
) -> ResponseJson<WorkspaceDetailsResponse> {
    let now = chrono::Utc::now().to_string();
    no_git_response(workspace_id, name, now, None, true)
}

pub async fn build_workspace_details_response(
    workspace_id: Uuid,
    name: &str,
    workspace_root: &std::path::Path,
    // Set to true in local mode so the response reports `GitMode::None`
    // even when a `.git` folder exists on disk. Opposite polarity from
    // the router's `include_git_features` — matches `ServeMode::is_local`.
    git_disabled: bool,
) -> Result<ResponseJson<WorkspaceDetailsResponse>, StatusCode> {
    let now = chrono::Utc::now().to_string();

    // Workspace directory doesn't exist on disk (e.g. deleted externally).
    // Return a flagged response with safe defaults so the frontend can
    // surface a toast instead of erroring.
    if !workspace_root.exists() {
        return Ok(no_git_response(
            workspace_id,
            name,
            now,
            Some(format!(
                "Workspace directory not found: {}",
                workspace_root.display()
            )),
            false,
        ));
    }

    // Local-mode servers disable all git features — routes are not
    // mounted, capabilities must match. Force None regardless of
    // what lives on disk.
    if git_disabled {
        return Ok(no_git_response(workspace_id, name, now, None, false));
    }

    let git = default_git_client();
    let git_mode = detect_git_mode(workspace_root).await;
    let has_local_repo = !matches!(git_mode, GitMode::None);

    let default_branch = if has_local_repo {
        git.get_default_branch(workspace_root).await
    } else {
        "main".to_string()
    };

    let current_branch = git
        .get_current_branch(workspace_root)
        .await
        .unwrap_or_else(|_| default_branch.clone());

    // Resolve protected_branches from config.yml; fall back to
    // [default_branch] on any error OR when there is no local repo.
    let protected_branches: Vec<String> = if has_local_repo {
        let config_branches = match ConfigBuilder::new().with_workspace_path(workspace_root) {
            Ok(builder) => match builder.build_with_fallback_config().await {
                Ok(manager) => manager.protected_branches().map(|b| b.to_vec()),
                Err(err) => {
                    tracing::warn!(
                        workspace_path = %workspace_root.display(),
                        error = %err,
                        "failed to build config for protected_branches; falling back to default"
                    );
                    None
                }
            },
            Err(err) => {
                tracing::warn!(
                    workspace_path = %workspace_root.display(),
                    error = %err,
                    "failed to build config for protected_branches; falling back to default"
                );
                None
            }
        };
        config_branches.unwrap_or_else(|| vec![default_branch.clone()])
    } else {
        vec![default_branch.clone()]
    };

    Ok(ResponseJson(WorkspaceDetailsResponse {
        id: workspace_id,
        name: name.to_string(),
        workspace_id: Uuid::nil(),
        created_at: now.clone(),
        updated_at: now.clone(),
        active_branch: Some(ProjectBranch {
            id: Uuid::nil(),
            name: current_branch,
            revision: String::new(),
            workspace_id,
            branch_type: BranchType::Local,
            sync_status: "synced".to_string(),
            created_at: now.clone(),
            updated_at: now,
        }),
        workspace_error: None,
        git_mode,
        capabilities: git_mode.into(),
        default_branch,
        protected_branches,
        requires_local_setup: false,
    }))
}

/// Get all branches for a specific workspace.
///
/// This endpoint retrieves all branches (both local and remote) associated with a workspace.
/// Returns branch metadata including names, revisions, sync status, and timestamps.
#[utoipa::path(
    get,
    path = "/workspaces/{workspace_id}/branches",
    params(
        ("workspace_id" = Uuid, Path, description = "Workspace ID")
    ),
    responses(
        (status = 200, description = "Branches retrieved successfully", body = WorkspaceBranchesResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Workspace not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Workspaces"
)]
pub async fn get_workspace_branches(
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Extension(ws): Extension<entity::workspaces::Model>,
) -> Result<ResponseJson<WorkspaceBranchesResponse>, StatusCode> {
    info!("Getting branches for workspace: {}", ws.id);

    let root = workspace_root(&ws).await?;
    let branches = git_list_branches(&root, ws.id).await;
    info!("Found {} branches", branches.len());
    Ok(ResponseJson(WorkspaceBranchesResponse { branches }))
}

pub async fn delete_branch(
    Extension(ws): Extension<entity::workspaces::Model>,
    Path((_workspace_id, branch_name)): Path<(Uuid, String)>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    let root = workspace_root(&ws).await?;
    match git_delete_branch(&root, &branch_name).await {
        Ok(()) => Ok(ResponseJson(ProjectResponse {
            success: true,
            message: format!("Branch '{}' deleted", branch_name),
        })),
        Err(e) => {
            error!("{}", e);
            Ok(ResponseJson(ProjectResponse {
                success: false,
                message: format!("{e}"),
            }))
        }
    }
}

pub async fn switch_workspace_branch(
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Extension(ws): Extension<entity::workspaces::Model>,
    Json(request): Json<SwitchBranchRequest>,
) -> Result<ResponseJson<ProjectBranch>, StatusCode> {
    info!("Switching branch for workspace: {}", ws.id);

    let root = workspace_root(&ws).await?;
    // Validate branch name before it reaches the shell.
    if let Err(e) = default_git_client().validate_branch_name(&request.branch) {
        error!("Invalid branch name '{}': {}", request.branch, e);
        return Err(StatusCode::BAD_REQUEST);
    }

    git_switch_branch(&root, &request.branch, ws.id)
        .await
        .map(ResponseJson)
        .map_err(|e| {
            error!("{}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProjectStatus {
    pub required_secrets: Option<Vec<String>>,
    pub is_config_valid: bool,
    pub error: Option<String>,
}

pub async fn get_workspace_status(
    State(_app_state): State<AppState>,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Extension(workspace): Extension<entity::workspaces::Model>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
) -> Result<axum::response::Json<ProjectStatus>, StatusCode> {
    use entity::workspaces::WorkspaceStatus;

    // Short-circuit when the row itself records a clone failure — the workspace
    // directory may be empty or partial, so running WorkspaceBuilder is pointless.
    if workspace.status == WorkspaceStatus::Failed {
        return Ok(axum::response::Json(ProjectStatus {
            required_secrets: None,
            is_config_valid: false,
            error: workspace.error,
        }));
    }

    let workspace_path = workspace_manager.config_manager.workspace_path();

    let (is_config_valid, required_secrets, error) = match WorkspaceBuilder::new(workspace_id)
        .with_workspace_path(&workspace_path)
        .await
    {
        Ok(_builder) => (true, Some(Vec::new()), None),
        Err(e) => {
            error!("Failed to create workspace builder: {}", e);
            (false, None, Some(e.to_string()))
        }
    };

    let status = ProjectStatus {
        required_secrets,
        is_config_valid,
        error,
    };

    Ok(axum::response::Json(status))
}

/// Summary of a registered workspace returned by `GET /orgs/{org_id}/workspaces`.
#[derive(Debug, Serialize)]
pub struct WorkspaceSummary {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub name: String,
    pub path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_opened_at: Option<DateTime<Utc>>,
    /// Display name of the user who created this workspace, if known.
    pub created_by_name: Option<String>,
    /// Number of `.agent.yml` files found (recursive).
    pub agent_count: usize,
    /// Number of `.workflow.yml` files found (recursive).
    pub workflow_count: usize,
    /// Number of `.app.yml` files found (recursive).
    pub app_count: usize,
    /// Git remote URL (e.g. `https://github.com/org/repo`), if set.
    pub git_remote: Option<String>,
    /// Short commit hash + message of HEAD, if available.
    pub git_commit: Option<String>,
    /// Human-readable relative date of the last commit (e.g. "3 hours ago").
    pub git_updated_at: Option<String>,
    pub status: entity::workspaces::WorkspaceStatus,
    pub error: Option<String>,
}

/// Count files whose name ends with `.<suffix>.yml` under `dir` (recursive, skips hidden dirs).
fn count_yml_suffix(dir: &std::path::Path, suffix: &str) -> usize {
    let pattern = format!(".{suffix}.yml");
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                let hidden = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with('.'))
                    .unwrap_or(false);
                if !hidden {
                    count += count_yml_suffix(&path, suffix);
                }
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with(&pattern))
                .unwrap_or(false)
            {
                count += 1;
            }
        }
    }
    count
}

/// GET /orgs/{org_id}/workspaces — list workspaces in the given org.
/// Membership is enforced by org_middleware; this handler just scopes the query.
pub async fn list_workspaces(
    crate::server::api::middlewares::org_context::OrgContextExtractor(ctx): crate::server::api::middlewares::org_context::OrgContextExtractor,
    State(app_state): State<AppState>,
) -> Result<ResponseJson<Vec<WorkspaceSummary>>, StatusCode> {
    use entity::prelude::Workspaces;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    let db = oxy::database::client::establish_connection()
        .await
        .map_err(|e| {
            error!("Failed to connect to database: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let workspaces = Workspaces::find()
        .filter(entity::workspaces::Column::OrgId.eq(ctx.org.id))
        .all(&db)
        .await
        .map_err(|e| {
            error!("Failed to list workspaces: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Batch-fetch creator names for all workspaces that have a created_by set.
    let creator_ids: Vec<Uuid> = workspaces
        .iter()
        .filter_map(|p| p.created_by)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let creator_names: std::collections::HashMap<Uuid, String> = if creator_ids.is_empty() {
        std::collections::HashMap::new()
    } else {
        use entity::prelude::Users;
        use sea_orm::{ColumnTrait, QueryFilter};
        Users::find()
            .filter(entity::users::Column::Id.is_in(creator_ids))
            .all(&db)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|u| {
                let display = if u.name.is_empty() { u.email } else { u.name };
                (u.id, display)
            })
            .collect()
    };

    let summary_futures = workspaces.into_iter().map(|p| {
        let creator_names = creator_names.clone();
        async move {
            // Gather file counts and git metadata when the path exists on disk.
            let (agent_count, workflow_count, app_count, git_remote, git_commit, git_updated_at) =
                if let Some(ref path_str) = p.path {
                    let dir = std::path::PathBuf::from(path_str);
                    if dir.exists() {
                        let (agent_count, workflow_count, app_count) =
                            tokio::task::spawn_blocking({
                                let dir = dir.clone();
                                move || {
                                    (
                                        count_yml_suffix(&dir, "agent"),
                                        count_yml_suffix(&dir, "workflow"),
                                        count_yml_suffix(&dir, "app"),
                                    )
                                }
                            })
                            .await
                            .unwrap_or((0, 0, 0));
                        let git = default_git_client();
                        let (remote, (sha, msg), updated_at) = tokio::join!(
                            git.get_remote_url(&dir),
                            git.get_branch_commit(&dir, "HEAD"),
                            git.get_head_commit_relative_date(&dir)
                        );
                        let commit = if sha.is_empty() {
                            None
                        } else {
                            Some(format!("{} — {}", &sha[..sha.len().min(7)], msg))
                        };
                        (
                            agent_count,
                            workflow_count,
                            app_count,
                            remote,
                            commit,
                            updated_at,
                        )
                    } else {
                        (0, 0, 0, None, None, None)
                    }
                } else {
                    (0, 0, 0, None, None, None)
                };

            WorkspaceSummary {
                id: p.id,
                org_id: p.org_id,
                name: p.name,
                path: p.path,
                created_at: p.created_at.into(),
                last_opened_at: p.last_opened_at.map(|t| t.into()),
                created_by_name: p.created_by.and_then(|id| creator_names.get(&id).cloned()),
                agent_count,
                workflow_count,
                app_count,
                git_remote,
                git_commit,
                git_updated_at,
                status: p.status,
                error: p.error,
            }
        }
    });

    let summaries = futures::future::join_all(summary_futures).await;

    Ok(ResponseJson(summaries))
}

/// Checks the authenticated user has owner or admin role in the workspace's org.
async fn validate_workspace_org_admin(
    workspace: &entity::workspaces::Model,
    user_id: uuid::Uuid,
    db: &sea_orm::DatabaseConnection,
) -> Result<(), StatusCode> {
    use entity::org_members::{Column as OmCol, OrgRole};
    use entity::prelude::OrgMembers;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    let Some(org_id) = workspace.org_id else {
        return Ok(());
    };

    let membership = OrgMembers::find()
        .filter(OmCol::OrgId.eq(org_id))
        .filter(OmCol::UserId.eq(user_id))
        .one(db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check org membership: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::FORBIDDEN)?;

    if !matches!(membership.role, OrgRole::Owner | OrgRole::Admin) {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(())
}

/// PATCH /workspaces/{id}/rename — change the display name of a workspace.
#[derive(Deserialize)]
pub struct RenameWorkspaceRequest {
    pub name: String,
}

pub async fn rename_workspace(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Path((org_id, workspace_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<RenameWorkspaceRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    use entity::prelude::Workspaces;
    use entity::workspaces;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, Set};

    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Workspace name cannot be empty".to_string(),
        ));
    }

    // Guard against control characters and non-printable input.
    if !name.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
        return Err((
            StatusCode::BAD_REQUEST,
            "Workspace name contains invalid characters".to_string(),
        ));
    }

    let db = oxy::database::client::establish_connection()
        .await
        .map_err(|e| {
            error!("Failed to connect to database: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database connection failed: {e}"),
            )
        })?;

    // Fetch the workspace first so we can check ownership before doing anything else.
    let workspace = Workspaces::find_by_id(workspace_id)
        .one(&db)
        .await
        .map_err(|e| {
            error!("Failed to find workspace {}: {}", workspace_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to find workspace: {e}"),
            )
        })?
        .ok_or((StatusCode::NOT_FOUND, "Workspace not found".to_string()))?;

    if workspace.org_id != Some(org_id) {
        return Err((StatusCode::NOT_FOUND, "Workspace not found".to_string()));
    }

    // Only org admins/owners or the workspace creator can rename.
    if let Some(org_id) = workspace.org_id {
        use entity::org_members::OrgRole;
        use entity::prelude::OrgMembers;
        let membership = OrgMembers::find()
            .filter(entity::org_members::Column::OrgId.eq(org_id))
            .filter(entity::org_members::Column::UserId.eq(user.id))
            .one(&db)
            .await
            .map_err(|e| {
                tracing::error!("Failed to check org membership: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "DB error".to_string())
            })?;
        let is_admin = membership
            .as_ref()
            .map(|m| matches!(m.role, OrgRole::Owner | OrgRole::Admin))
            .unwrap_or(false);
        let is_member = membership.is_some();
        let is_creator = workspace.created_by == Some(user.id);
        if !is_member {
            return Err((StatusCode::FORBIDDEN, "Not an org member".to_string()));
        }
        if !is_admin && !is_creator {
            return Err((
                StatusCode::FORBIDDEN,
                "Only admins or workspace creator can rename".to_string(),
            ));
        }
    } else {
        // Legacy workspaces (no org): only the creator can rename.
        if workspace.created_by != Some(user.id) {
            return Err((
                StatusCode::FORBIDDEN,
                "Only the workspace creator can rename".to_string(),
            ));
        }
    }

    // Reject duplicate names within the same org (or globally for legacy workspaces).
    let mut name_query = Workspaces::find()
        .filter(workspaces::Column::Name.eq(&name))
        .filter(workspaces::Column::Id.ne(workspace_id));
    if let Some(org_id) = workspace.org_id {
        name_query = name_query.filter(workspaces::Column::OrgId.eq(org_id));
    }
    let name_taken = name_query
        .one(&db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query workspaces: {e}"),
            )
        })?
        .is_some();

    if name_taken {
        return Err((
            StatusCode::CONFLICT,
            format!("A workspace named '{name}' already exists. Please choose a different name."),
        ));
    }

    let mut active: workspaces::ActiveModel = workspace.into_active_model();
    active.name = Set(name.clone());
    active.updated_at = Set(chrono::Utc::now().into());
    active.save(&db).await.map_err(|e| {
        error!("Failed to rename workspace {}: {}", workspace_id, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to rename workspace: {e}"),
        )
    })?;

    info!("Renamed workspace {} to '{}'", workspace_id, name);
    Ok(StatusCode::OK)
}

/// DELETE /workspaces/{id} — remove a workspace record from the database.
///
/// Pass `?delete_files=true` to also remove the workspace directory from disk.
/// Without that flag only the DB record is removed, leaving files intact.
/// Requires Admin or Owner role.
#[derive(Deserialize)]
pub struct DeleteProjectQuery {
    #[serde(default)]
    pub delete_files: bool,
}

pub async fn delete_workspace(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    State(app_state): State<AppState>,
    Path((org_id, workspace_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<DeleteProjectQuery>,
) -> Result<StatusCode, StatusCode> {
    use entity::prelude::Workspaces;
    use sea_orm::{EntityTrait, ModelTrait};

    let db = oxy::database::client::establish_connection()
        .await
        .map_err(|e| {
            error!("Failed to connect to database: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let workspace = Workspaces::find_by_id(workspace_id)
        .one(&db)
        .await
        .map_err(|e| {
            error!("Failed to find workspace {}: {}", workspace_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if workspace.org_id != Some(org_id) {
        return Err(StatusCode::NOT_FOUND);
    }

    // Org-scoped workspaces: require org admin/owner.
    // Legacy workspaces (no org): require workspace creator.
    if workspace.org_id.is_some() {
        validate_workspace_org_admin(&workspace, user.id, &db).await?;
    } else if workspace.created_by != Some(user.id) {
        return Err(StatusCode::FORBIDDEN);
    }

    // Capture the workspace path before deleting the record
    let workspace_path = effective_workspace_path(&workspace, None).await.ok();

    workspace.delete(&db).await.map_err(|e| {
        error!("Failed to delete workspace {}: {}", workspace_id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Deleted workspace {}", workspace_id);

    // Only remove files from disk when the caller explicitly opts in.
    // Without `?delete_files=true` we only remove the DB record, leaving
    // the directory intact so an accidental delete can be recovered.
    if query.delete_files
        && let Some(path) = workspace_path
        && path.exists()
    {
        if let Err(e) = std::fs::remove_dir_all(&path) {
            tracing::warn!("Failed to delete workspace directory {:?}: {}", path, e);
        } else {
            info!("Deleted workspace directory {:?}", path);
        }
    }

    Ok(StatusCode::OK)
}
