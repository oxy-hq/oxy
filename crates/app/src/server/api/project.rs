use axum::extract::State;
use entity::settings::SyncStatus;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

use crate::server::api::middlewares::project::{BranchQuery, ProjectManagerExtractor, ProjectPath};
use crate::server::router::AppState;
use crate::server::service::project::ProjectService;
use crate::server::service::secret_manager::SecretManagerService;
use oxy::adapters::project::builder::ProjectBuilder;
use oxy::adapters::secrets::SecretsManager;
use oxy::api_types::{BranchType, ProjectBranch, RecentCommitsResponse, RevisionInfoResponse};
use oxy::config::resolve_local_project_path;
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_project::LocalGitService;

use entity::{prelude::WorkspaceUsers, workspace_users};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

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
pub struct ProjectDetailsResponse {
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
pub struct ProjectBranchesResponse {
    pub branches: Vec<ProjectBranch>,
}

pub async fn pull_changes(
    State(app_state): State<AppState>,
    Query(query): Query<BranchQuery>,
    Path(project_id): Path<Uuid>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    match app_state
        .backend
        .pull(project_id, query.branch.clone())
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
    Path(_project_id): Path<Uuid>,
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
    Path(_project_id): Path<Uuid>,
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
    Path(_project_id): Path<Uuid>,
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
    Path(_project_id): Path<Uuid>,
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
    Path(_project_id): Path<Uuid>,
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
    Path(_project_id): Path<Uuid>,
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
    Path(_project_id): Path<Uuid>,
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
    Path(_project_id): Path<Uuid>,
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
    Path(project_id): Path<Uuid>,
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
        .push(project_id, query.branch.clone(), commit_message)
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
    Path(project_id): Path<Uuid>,
    Query(query): Query<BranchQuery>,
) -> Result<ResponseJson<RevisionInfoResponse>, StatusCode> {
    app_state
        .backend
        .revision_info(project_id, query.branch.as_deref())
        .await
        .map(ResponseJson)
        .map_err(|e| {
            error!("{}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// Get detailed information about a specific project including its active branch.
///
/// This endpoint retrieves complete project details along with the currently active branch information.
/// Requires authentication and returns project metadata, provider info, and active branch details.
#[utoipa::path(
    get,
    path = "/projects/{project_id}",
    params(
        ("project_id" = Uuid, Path, description = "Project ID")
    ),
    responses(
        (status = 200, description = "Project details retrieved successfully", body = ProjectDetailsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Project not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Projects"
)]
pub async fn get_project(
    State(app_state): State<AppState>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path(project_id): Path<Uuid>,
) -> Result<ResponseJson<ProjectDetailsResponse>, StatusCode> {
    info!("Getting project details for ID: {}", project_id);

    // In local mode, build a synthetic project from the local git state.
    if !app_state.cloud {
        let project_root = resolve_local_project_path().map_err(|e| {
            error!("Failed to resolve local project path: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        let default_branch = LocalGitService::get_default_branch(&project_root).await;
        let current_branch = LocalGitService::get_current_branch(&project_root)
            .await
            .unwrap_or_else(|_| default_branch.clone());
        let now = chrono::Utc::now().to_string();
        return Ok(ResponseJson(ProjectDetailsResponse {
            id: project_id,
            name: "Oxy".to_string(),
            workspace_id: Uuid::nil(),
            project_repo_id: None,
            created_at: now.clone(),
            updated_at: now.clone(),
            active_branch: Some(ProjectBranch {
                id: Uuid::nil(),
                name: current_branch,
                revision: String::new(),
                project_id,
                branch_type: BranchType::Local,
                sync_status: "synced".to_string(),
                created_at: now.clone(),
                updated_at: now,
            }),
        }));
    }

    let project = match ProjectService::get_project(project_id).await {
        Ok(Some(project)) => project,
        Ok(None) => {
            error!("Project not found: {}", project_id);
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            error!("Failed to get project: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let active_branch = match ProjectService::get_branch(project.active_branch_id).await {
        Ok(Some(branch)) => branch,
        Ok(None) => {
            error!("Branch not found: {}", project.active_branch_id);
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            error!("Failed to get branch: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    Ok(ResponseJson(ProjectDetailsResponse {
        id: project.id,
        name: project.name,
        workspace_id: project.workspace_id,
        project_repo_id: project.project_repo_id,
        created_at: project.created_at.to_string(),
        updated_at: project.updated_at.to_string(),
        active_branch: Some(ProjectBranch {
            id: active_branch.id,
            name: active_branch.name,
            revision: active_branch.revision,
            project_id: active_branch.project_id,
            branch_type: BranchType::Local,
            sync_status: active_branch.sync_status.to_string(),
            created_at: active_branch.created_at.to_string(),
            updated_at: active_branch.updated_at.to_string(),
        }),
    }))
}

/// Get all branches for a specific project.
///
/// This endpoint retrieves all branches (both local and remote) associated with a project.
/// Returns branch metadata including names, revisions, sync status, and timestamps.
#[utoipa::path(
    get,
    path = "/projects/{project_id}/branches",
    params(
        ("project_id" = Uuid, Path, description = "Project ID")
    ),
    responses(
        (status = 200, description = "Branches retrieved successfully", body = ProjectBranchesResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Project not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Projects"
)]
pub async fn get_project_branches(
    State(app_state): State<AppState>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path(project_id): Path<Uuid>,
) -> Result<ResponseJson<ProjectBranchesResponse>, StatusCode> {
    info!("Getting branches for project: {}", project_id);

    app_state
        .backend
        .list_branches(project_id)
        .await
        .map(|branches| {
            info!("Found {} branches", branches.len());
            ResponseJson(ProjectBranchesResponse { branches })
        })
        .map_err(|e| {
            error!("{}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn delete_branch(
    State(app_state): State<AppState>,
    Path((project_id, branch_name)): Path<(Uuid, String)>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    match app_state
        .backend
        .delete_branch(project_id, &branch_name)
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

pub async fn switch_project_branch(
    State(app_state): State<AppState>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path(project_id): Path<Uuid>,
    Json(request): Json<SwitchBranchRequest>,
) -> Result<ResponseJson<ProjectBranch>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    info!("Switching branch for project: {}", project_id);

    app_state
        .backend
        .switch_branch(project_id, &request.branch)
        .await
        .map(ResponseJson)
        .map_err(|e| {
            error!("{}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn switch_project_active_branch(
    State(app_state): State<AppState>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path(project_id): Path<Uuid>,
    Json(request): Json<SwitchBranchRequest>,
) -> Result<ResponseJson<ProjectBranch>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    info!("Getting switched active branch for project: {}", project_id);

    match ProjectService::switch_project_active_branch(project_id, request.branch).await {
        Ok(branch) => {
            info!(
                "Successfully switched active branch for project {}",
                project_id
            );
            Ok(ResponseJson(ProjectBranch {
                id: branch.id,
                project_id,
                branch_type: BranchType::Local,
                name: branch.name,
                revision: branch.revision,
                sync_status: branch.sync_status,
                created_at: branch.created_at.to_string(),
                updated_at: branch.updated_at.to_string(),
            }))
        }
        Err(e) => {
            error!("Failed to get branches for project {}: {}", project_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Delete a project from a workspace.
///
/// This endpoint permanently removes a project and all its associated data from the workspace.
/// Only workspace owners and admins are authorized to delete projects. The operation is irreversible.
#[utoipa::path(
    delete,
    path = "/projects/{project_id}",
    params(
        ("project_id" = Uuid, Path, description = "Project ID")
    ),
    responses(
        (status = 200, description = "Project deleted successfully", body = ProjectResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Project not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Projects"
)]
pub async fn delete_project(
    AuthenticatedUserExtractor(requester): AuthenticatedUserExtractor,
    Path((workspace_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    info!("Deleting project: {}", project_id);

    let db = establish_connection().await.map_err(|e| {
        error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let requester_role = WorkspaceUsers::find()
        .filter(workspace_users::Column::WorkspaceId.eq(workspace_id))
        .filter(workspace_users::Column::UserId.eq(requester.id))
        .one(&db)
        .await
        .map_err(|e| {
            error!("Failed to query requester role: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(|ou| ou.role);
    if requester_role.as_deref() != Some("owner") && requester_role.as_deref() != Some("admin") {
        return Err(StatusCode::FORBIDDEN);
    }

    match ProjectService::delete_project(project_id).await {
        Ok(_) => {
            info!("Project {} deleted successfully", project_id);
            Ok(ResponseJson(ProjectResponse {
                success: true,
                message: "Project deleted successfully".to_string(),
            }))
        }
        Err(e) => {
            error!("Failed to delete project {}: {}", project_id, e);
            Ok(ResponseJson(ProjectResponse {
                success: false,
                message: format!("Failed to delete project: {e}"),
            }))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProjectStatus {
    pub required_secrets: Option<Vec<String>>,
    pub is_config_valid: bool,
    pub error: Option<String>,
}

pub async fn get_project_status(
    State(app_state): State<AppState>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path(ProjectPath { project_id }): Path<ProjectPath>,
) -> Result<axum::response::Json<ProjectStatus>, StatusCode> {
    let project_path = project_manager.config_manager.project_path();

    let (is_config_valid, required_secrets, error) = match ProjectBuilder::new(project_id)
        .with_project_path(&project_path)
        .await
    {
        Ok(builder) => {
            if !app_state.cloud {
                (true, Some(Vec::new()), None)
            } else {
                let secrets_manager =
                    match SecretsManager::from_database(SecretManagerService::new(project_id)) {
                        Ok(sm) => sm,
                        Err(e) => {
                            error!("Failed to create secrets manager: {}", e);
                            return Err(StatusCode::INTERNAL_SERVER_ERROR);
                        }
                    };

                match builder.with_secrets_manager(secrets_manager).build().await {
                    Ok(config) => {
                        let secrets = match config.get_required_secrets().await {
                            Ok(secrets) => secrets,
                            Err(e) => {
                                error!("Failed to get required secrets: {}", e);
                                None
                            }
                        };
                        (true, secrets, None)
                    }
                    Err(e) => {
                        error!("Failed to build config: {}", e);
                        (false, None, Some(e.to_string()))
                    }
                }
            }
        }
        Err(e) => {
            error!("Failed to create project builder: {}", e);
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

/// Simple health check endpoint to test routing
#[utoipa::path(
    get,
    path = "/workspaces/{workspace_id}/projects/health",
    params(
        ("workspace_id" = Uuid, Path, description = "Workspace ID")
    ),
    responses(
        (status = 200, description = "Health check successful", body = String),
    ),
    tag = "Projects"
)]
pub async fn project_health_check(
    Path(workspace_id): Path<Uuid>,
) -> Result<ResponseJson<String>, StatusCode> {
    info!("Health check for workspace: {}", workspace_id);
    Ok(ResponseJson("OK".to_string()))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateRepoFromProjectRequest {
    pub git_namespace_id: Uuid,
    pub repo_name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateRepoFromProjectResponse {
    pub success: bool,
    pub message: String,
}

#[utoipa::path(
    post,
    path = "/projects/{project_id}/create-repo",
    request_body = CreateRepoFromProjectRequest,
    params(
        ("project_id" = Uuid, Path, description = "Project ID")
    ),
    responses(
        (status = 200, description = "Repository created successfully", body = CreateRepoFromProjectResponse),
        (status = 400, description = "Bad request - project already has repository or invalid parameters"),
        (status = 404, description = "Project or git namespace not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Projects"
)]
pub async fn create_repo_from_project(
    Path(project_id): Path<Uuid>,
    Json(request): Json<CreateRepoFromProjectRequest>,
) -> Result<ResponseJson<CreateRepoFromProjectResponse>, StatusCode> {
    info!(
        "Creating repository '{}' for project {} using git namespace {}",
        request.repo_name, project_id, request.git_namespace_id
    );

    match crate::service::project::project_operations::ProjectService::create_repo_from_project(
        project_id,
        request.git_namespace_id,
        request.repo_name.clone(),
    )
    .await
    {
        Ok(()) => {
            info!("Successfully created repository for project {}", project_id);
            Ok(ResponseJson(CreateRepoFromProjectResponse {
                success: true,
                message: format!(
                    "Repository '{}' created and linked to project successfully",
                    request.repo_name
                ),
            }))
        }
        Err(e) => {
            error!(
                "Failed to create repository for project {}: {}",
                project_id, e
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
