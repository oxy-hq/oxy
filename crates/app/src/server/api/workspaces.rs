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
use crate::server::service::project::WorkspaceService;
use oxy::adapters::workspace::builder::WorkspaceBuilder;
use oxy::api_types::{BranchType, ProjectBranch, RecentCommitsResponse, RevisionInfoResponse};
use oxy::config::resolve_local_workspace_path;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_project::LocalGitService;

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
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProjectResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkspaceDetailsResponse {
    pub id: Uuid,
    pub name: String,
    pub workspace_id: Uuid,
    pub project_repo_id: Option<Uuid>,
    pub active_branch: Option<ProjectBranch>,
    pub created_at: String,
    pub updated_at: String,
}

// BranchType and ProjectBranch imported from oxy::api_types

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkspaceBranchesResponse {
    pub branches: Vec<ProjectBranch>,
}

pub async fn pull_changes(
    State(app_state): State<AppState>,
    Query(query): Query<BranchQuery>,
    Path(workspace_id): Path<Uuid>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    match app_state
        .backend
        .pull(workspace_id, query.branch.clone())
        .await
    {
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
    State(app_state): State<AppState>,
    Path(_workspace_id): Path<Uuid>,
    Query(query): Query<ResolveConflictWithContentQuery>,
    Json(body): Json<ResolveConflictWithContentBody>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    match app_state
        .backend
        .resolve_conflict_with_content(query.branch.as_deref(), &query.file, &body.content)
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
    State(app_state): State<AppState>,
    Path(_workspace_id): Path<Uuid>,
    Query(query): Query<ResolveConflictQuery>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    let use_mine = query.side == "mine";
    match app_state
        .backend
        .resolve_conflict_file(query.branch.as_deref(), &query.file, use_mine)
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
    State(app_state): State<AppState>,
    Path(_workspace_id): Path<Uuid>,
    Query(query): Query<UnresolveConflictQuery>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    match app_state
        .backend
        .unresolve_conflict_file(query.branch.as_deref(), &query.file)
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
    State(app_state): State<AppState>,
    Path(_workspace_id): Path<Uuid>,
    Query(query): Query<BranchQuery>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    match app_state.backend.force_push(query.branch.as_deref()).await {
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
    State(app_state): State<AppState>,
    Path(_workspace_id): Path<Uuid>,
    Query(query): Query<BranchQuery>,
) -> Result<ResponseJson<RecentCommitsResponse>, StatusCode> {
    let result = app_state
        .backend
        .get_recent_commits(query.branch.as_deref(), 10)
        .await;
    Ok(ResponseJson(result))
}

pub async fn reset_to_commit(
    State(app_state): State<AppState>,
    Path(_workspace_id): Path<Uuid>,
    Query(query): Query<ResetToCommitQuery>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    match app_state
        .backend
        .reset_to_commit(query.branch.as_deref(), &query.commit)
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
    State(app_state): State<AppState>,
    Path(_workspace_id): Path<Uuid>,
    Query(query): Query<BranchQuery>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    match app_state
        .backend
        .abort_rebase(query.branch.as_deref())
        .await
    {
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
    State(app_state): State<AppState>,
    Path(_workspace_id): Path<Uuid>,
    Query(query): Query<BranchQuery>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    match app_state
        .backend
        .continue_rebase(query.branch.as_deref())
        .await
    {
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
    State(app_state): State<AppState>,
    Query(query): Query<BranchQuery>,
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<PushChangesRequest>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    let commit_message = request
        .commit_message
        .clone()
        .unwrap_or_else(|| "Auto-commit: Oxy changes".to_string());

    match app_state
        .backend
        .push(workspace_id, query.branch.clone(), commit_message)
        .await
    {
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
    State(app_state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
    Query(query): Query<BranchQuery>,
) -> Result<ResponseJson<RevisionInfoResponse>, StatusCode> {
    app_state
        .backend
        .revision_info(workspace_id, query.branch.as_deref())
        .await
        .map(ResponseJson)
        .map_err(|e| {
            error!("{}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
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
    Path(workspace_id): Path<Uuid>,
) -> Result<ResponseJson<WorkspaceDetailsResponse>, StatusCode> {
    info!("Getting workspace details for ID: {}", workspace_id);

    // For a real (non-nil) UUID: look up from DB.
    if workspace_id != Uuid::nil() {
        use entity::prelude::Workspaces;
        use sea_orm::EntityTrait;
        let db = oxy::database::client::establish_connection()
            .await
            .map_err(|e| {
                error!("DB connection failed: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        let project = Workspaces::find_by_id(workspace_id)
            .one(&db)
            .await
            .map_err(|e| {
                error!("DB query failed: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or(StatusCode::NOT_FOUND)?;

        let workspace_root = project
            .path
            .as_deref()
            .map(std::path::PathBuf::from)
            .ok_or_else(|| {
                error!("Project {} has no path", workspace_id);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        return build_workspace_details_response(workspace_id, &project.name, &workspace_root)
            .await;
    }

    // Nil UUID bootstrap: resolve via active_workspace_path, return real UUID if found in DB.
    let workspace_root = {
        let locked = app_state.active_workspace_path.read().await;
        locked.clone()
    }
    .or_else(|| resolve_local_workspace_path().ok())
    .ok_or_else(|| {
        error!("No active workspace path configured");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Try to find the real workspace UUID in DB by matching path.
    let resolved_id = {
        use entity::prelude::Workspaces;
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
        if let Ok(db) = oxy::database::client::establish_connection().await {
            let path_str = workspace_root.to_string_lossy().to_string();
            Workspaces::find()
                .filter(entity::workspaces::Column::Path.eq(path_str))
                .one(&db)
                .await
                .ok()
                .flatten()
                .map(|p| (p.id, p.name))
        } else {
            None
        }
    };

    let (real_id, name) = resolved_id.unwrap_or((Uuid::nil(), "Oxy".to_string()));

    build_workspace_details_response(real_id, &name, &workspace_root).await
}

async fn build_workspace_details_response(
    workspace_id: Uuid,
    name: &str,
    workspace_root: &std::path::Path,
) -> Result<ResponseJson<WorkspaceDetailsResponse>, StatusCode> {
    let default_branch = LocalGitService::get_default_branch(workspace_root).await;
    let current_branch = LocalGitService::get_current_branch(workspace_root)
        .await
        .unwrap_or_else(|_| default_branch.clone());
    let now = chrono::Utc::now().to_string();
    Ok(ResponseJson(WorkspaceDetailsResponse {
        id: workspace_id,
        name: name.to_string(),
        workspace_id: Uuid::nil(),
        project_repo_id: None,
        created_at: now.clone(),
        updated_at: now.clone(),
        active_branch: Some(ProjectBranch {
            id: Uuid::nil(),
            name: current_branch,
            revision: String::new(),
            workspace_id: workspace_id,
            branch_type: BranchType::Local,
            sync_status: "synced".to_string(),
            created_at: now.clone(),
            updated_at: now,
        }),
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
    State(app_state): State<AppState>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path(workspace_id): Path<Uuid>,
) -> Result<ResponseJson<WorkspaceBranchesResponse>, StatusCode> {
    info!("Getting branches for workspace: {}", workspace_id);

    app_state
        .backend
        .list_branches(workspace_id)
        .await
        .map(|branches| {
            info!("Found {} branches", branches.len());
            ResponseJson(WorkspaceBranchesResponse { branches })
        })
        .map_err(|e| {
            error!("{}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn delete_branch(
    State(app_state): State<AppState>,
    Path((workspace_id, branch_name)): Path<(Uuid, String)>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    match app_state
        .backend
        .delete_branch(workspace_id, &branch_name)
        .await
    {
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
    State(app_state): State<AppState>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<SwitchBranchRequest>,
) -> Result<ResponseJson<ProjectBranch>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    info!("Switching branch for workspace: {}", workspace_id);

    app_state
        .backend
        .switch_branch(workspace_id, &request.branch)
        .await
        .map(ResponseJson)
        .map_err(|e| {
            error!("{}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn switch_workspace_active_branch(
    State(app_state): State<AppState>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<SwitchBranchRequest>,
) -> Result<ResponseJson<ProjectBranch>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    info!(
        "Getting switched active branch for project: {}",
        workspace_id
    );

    match WorkspaceService::switch_project_active_branch(workspace_id, request.branch).await {
        Ok(branch) => {
            info!(
                "Successfully switched active branch for workspace {}",
                workspace_id
            );
            Ok(ResponseJson(ProjectBranch {
                id: branch.id,
                workspace_id: workspace_id,
                branch_type: BranchType::Local,
                name: branch.name,
                revision: branch.revision,
                sync_status: branch.sync_status,
                created_at: branch.created_at.to_string(),
                updated_at: branch.updated_at.to_string(),
            }))
        }
        Err(e) => {
            error!("Failed to get branches for project {}: {}", workspace_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProjectStatus {
    pub required_secrets: Option<Vec<String>>,
    pub is_config_valid: bool,
    pub error: Option<String>,
}

pub async fn get_workspace_status(
    State(app_state): State<AppState>,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path(WorkspacePath {
        workspace_id: workspace_id,
    }): Path<WorkspacePath>,
) -> Result<axum::response::Json<ProjectStatus>, StatusCode> {
    // Check if this workspace has a recorded clone error (not an Oxy project).
    if let Some(clone_err) = app_state
        .errored_workspaces
        .lock()
        .unwrap()
        .get(&workspace_id)
        .cloned()
    {
        return Ok(axum::response::Json(ProjectStatus {
            required_secrets: None,
            is_config_valid: false,
            error: Some(clone_err),
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateRepoFromWorkspaceRequest {
    pub git_namespace_id: Uuid,
    pub repo_name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateRepoFromWorkspaceResponse {
    pub success: bool,
    pub message: String,
}

#[utoipa::path(
    post,
    path = "/workspaces/{workspace_id}/create-repo",
    request_body = CreateRepoFromWorkspaceRequest,
    params(
        ("workspace_id" = Uuid, Path, description = "Workspace ID")
    ),
    responses(
        (status = 200, description = "Repository created successfully", body = CreateRepoFromWorkspaceResponse),
        (status = 400, description = "Bad request - workspace already has repository or invalid parameters"),
        (status = 404, description = "Workspace or git namespace not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Workspaces"
)]
pub async fn create_repo_from_workspace(
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<CreateRepoFromWorkspaceRequest>,
) -> Result<ResponseJson<CreateRepoFromWorkspaceResponse>, StatusCode> {
    info!(
        "Creating repository '{}' for workspace {} using git namespace {}",
        request.repo_name, workspace_id, request.git_namespace_id
    );

    match crate::service::project::workspace_operations::WorkspaceService::create_repo_from_project(
        workspace_id,
        request.git_namespace_id,
        request.repo_name.clone(),
    )
    .await
    {
        Ok(()) => {
            info!(
                "Successfully created repository for workspace {}",
                workspace_id
            );
            Ok(ResponseJson(CreateRepoFromWorkspaceResponse {
                success: true,
                message: format!(
                    "Repository '{}' created and linked to workspace successfully",
                    request.repo_name
                ),
            }))
        }
        Err(e) => {
            error!(
                "Failed to create repository for workspace {}: {}",
                workspace_id, e
            );
            let status_code = match e {
                oxy_shared::errors::OxyError::RuntimeError(ref msg)
                    if msg.contains("already has a repository") =>
                {
                    StatusCode::BAD_REQUEST
                }
                oxy_shared::errors::OxyError::RuntimeError(ref msg)
                    if msg.contains("not found") =>
                {
                    StatusCode::NOT_FOUND
                }
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };

            Err(status_code)
        }
    }
}

/// Summary of a registered workspace returned by `GET /workspaces`.
#[derive(Debug, Serialize)]
pub struct WorkspaceSummary {
    pub id: Uuid,
    pub name: String,
    pub path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_opened_at: Option<DateTime<Utc>>,
    pub active: bool,
    /// Display name of the user who created this workspace, if known.
    pub created_by_name: Option<String>,
    /// True while the background git clone for this workspace is still running.
    pub is_cloning: bool,
    /// Set when the clone completed but the repository is not an Oxy project (no config.yml found).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clone_error: Option<String>,
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

/// GET /workspaces — list all registered workspaces.
pub async fn list_workspaces(
    _user: AuthenticatedUserExtractor,
    State(app_state): State<AppState>,
) -> Result<ResponseJson<Vec<WorkspaceSummary>>, StatusCode> {
    use entity::prelude::Workspaces;
    use sea_orm::EntityTrait;

    let db = oxy::database::client::establish_connection()
        .await
        .map_err(|e| {
            error!("Failed to connect to database: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let active_path = {
        let locked = app_state.active_workspace_path.read().await;
        locked.clone()
    };

    let cloning_ids = app_state.cloning_workspaces.lock().unwrap().clone();
    let errored_ids = app_state.errored_workspaces.lock().unwrap().clone();

    let workspaces = Workspaces::find().all(&db).await.map_err(|e| {
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
        let active_path = active_path.clone();
        let cloning_ids = cloning_ids.clone();
        let errored_ids = errored_ids.clone();
        let creator_names = creator_names.clone();
        async move {
            let is_active = active_path
                .as_ref()
                .and_then(|ap| {
                    p.path
                        .as_ref()
                        .map(|pp| ap == &std::path::PathBuf::from(pp))
                })
                .unwrap_or(false);

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
                        let (remote, (sha, msg), updated_at) = tokio::join!(
                            LocalGitService::get_remote_url(&dir),
                            LocalGitService::get_branch_commit(&dir, "HEAD"),
                            LocalGitService::get_head_commit_relative_date(&dir)
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
                is_cloning: cloning_ids.contains(&p.id),
                clone_error: errored_ids.get(&p.id).cloned(),
                id: p.id,
                name: p.name,
                path: p.path,
                created_at: p.created_at.into(),
                last_opened_at: p.last_opened_at.map(|t| t.into()),
                active: is_active,
                created_by_name: p.created_by.and_then(|id| creator_names.get(&id).cloned()),
                agent_count,
                workflow_count,
                app_count,
                git_remote,
                git_commit,
                git_updated_at,
            }
        }
    });

    let summaries = futures::future::join_all(summary_futures).await;

    Ok(ResponseJson(summaries))
}

/// POST /workspaces/{id}/activate — make a workspace the active one.
pub async fn activate_workspace(
    _user: AuthenticatedUserExtractor,
    State(app_state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    use entity::prelude::Workspaces;
    use entity::workspaces;
    use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set};

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

    let path = workspace
        .path
        .clone()
        .map(std::path::PathBuf::from)
        .ok_or_else(|| {
            error!("Workspace {} has no path set", workspace_id);
            StatusCode::UNPROCESSABLE_ENTITY
        })?;

    if !path.exists() {
        error!("Workspace path {:?} does not exist on disk", path);
        return Err(StatusCode::NOT_FOUND);
    }

    // Persist last_opened_at so scan_and_register_workspaces can restore the
    // correct active workspace on the next server restart.
    let mut active: workspaces::ActiveModel = workspace.into_active_model();
    active.last_opened_at = Set(Some(chrono::Utc::now().into()));
    active.save(&db).await.map_err(|e| {
        error!(
            "Failed to update last_opened_at for workspace {}: {}",
            workspace_id, e
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut locked = app_state.active_workspace_path.write().await;
    *locked = Some(path);
    info!("Activated workspace {}", workspace_id);

    Ok(StatusCode::OK)
}

/// PATCH /workspaces/{id}/rename — change the display name of a workspace.
#[derive(Deserialize)]
pub struct RenameWorkspaceRequest {
    pub name: String,
}

pub async fn rename_workspace(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Path(workspace_id): Path<Uuid>,
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

    // Only the workspace creator or an admin can rename it.
    if workspace.created_by != Some(user.id) && !user.role.is_admin_or_above() {
        return Err((
            StatusCode::FORBIDDEN,
            "Insufficient permissions".to_string(),
        ));
    }

    // Reject duplicate names.
    let name_taken = Workspaces::find()
        .filter(workspaces::Column::Name.eq(&name))
        .filter(workspaces::Column::Id.ne(workspace_id))
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
    Path(workspace_id): Path<Uuid>,
    Query(query): Query<DeleteProjectQuery>,
) -> Result<StatusCode, StatusCode> {
    if !user.role.is_admin_or_above() {
        return Err(StatusCode::FORBIDDEN);
    }
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

    // Capture the workspace path before deleting the record
    let workspace_path = workspace.path.clone().map(std::path::PathBuf::from);

    workspace.delete(&db).await.map_err(|e| {
        error!("Failed to delete workspace {}: {}", workspace_id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Deleted workspace {}", workspace_id);

    // If the deleted workspace was the active one, clear active_workspace_path
    if let Some(ref path) = workspace_path {
        let mut locked = app_state.active_workspace_path.write().await;
        if locked.as_deref() == Some(path.as_path()) {
            *locked = None;
        }
    }

    // Clear any errored state for this workspace so the map doesn't grow unboundedly.
    app_state
        .errored_workspaces
        .lock()
        .unwrap()
        .remove(&workspace_id);

    // Only remove files from disk when the caller explicitly opts in.
    // Without `?delete_files=true` we only remove the DB record, leaving
    // the directory intact so an accidental delete can be recovered.
    if query.delete_files {
        if let Some(path) = workspace_path
            && path.exists()
        {
            if let Err(e) = std::fs::remove_dir_all(&path) {
                tracing::warn!("Failed to delete workspace directory {:?}: {}", path, e);
            } else {
                info!("Deleted workspace directory {:?}", path);
            }
        }
    }

    Ok(StatusCode::OK)
}
