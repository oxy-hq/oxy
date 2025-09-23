use entity::settings::SyncStatus;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

use crate::adapters::project::builder::ProjectBuilder;
use crate::adapters::secrets::SecretsManager;
use crate::api::middlewares::project::{BranchQuery, ProjectManagerExtractor, ProjectPath};
use crate::db::client::establish_connection;
use crate::github::{GitHubRepository, GithubBranch};
use crate::service::project_service::ProjectService;
use crate::service::secret_manager::SecretManagerService;
use crate::{auth::extractor::AuthenticatedUserExtractor, github::GitHubClient};

use entity::{organization_users, prelude::OrganizationUsers};
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
pub struct GitHubTokenQuery {
    pub token: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ListBranchesQuery {
    pub token: String,
    pub repo_full_name: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateProjectRequest {
    pub repo_id: i64,
    pub token: String,
    pub branch: String,
    pub provider: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SwitchBranchRequest {
    pub branch: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateProjectResponse {
    pub success: bool,
    pub message: String,
    pub project_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub local_path: Option<String>,
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
    pub organization_id: Uuid,
    pub provider: Option<String>,
    pub active_branch: Option<ProjectBranch>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BranchType {
    Remote,
    Local,
}

#[derive(Debug, Serialize, ToSchema)]
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

#[derive(Debug, Serialize, ToSchema)]
pub struct ProjectBranchesResponse {
    pub branches: Vec<ProjectBranch>,
}

#[utoipa::path(
    get,
    path = "/github/repositories",
    params(
        ("token" = String, Query, description = "GitHub access token")
    ),
    responses(
        (status = 200, description = "List of repositories retrieved successfully", body = Vec<GitHubRepository>),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Projects"
)]
pub async fn list_repositories(
    Query(query): Query<GitHubTokenQuery>,
) -> Result<Json<Vec<GitHubRepository>>, StatusCode> {
    let client = match GitHubClient::new(query.token.clone()) {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to create GitHub client: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    match client.list_repositories().await {
        Ok(repositories) => Ok(Json(repositories)),
        Err(e) => {
            error!("Failed to fetch repositories: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_branches(
    Query(query): Query<ListBranchesQuery>,
) -> Result<Json<Vec<GithubBranch>>, StatusCode> {
    let client = match GitHubClient::new(query.token.clone()) {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to create GitHub client: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    match client.list_branches(query.repo_full_name).await {
        Ok(branches) => Ok(Json(branches)),
        Err(e) => {
            error!("Failed to fetch branches: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[utoipa::path(
    post,
    path = "/organizations/{organization_id}/projects",
    params(
        ("organization_id" = Uuid, Path, description = "Organization ID")
    ),
    request_body = CreateProjectRequest,
    responses(
        (status = 200, description = "Project created successfully", body = CreateProjectResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Projects"
)]
pub async fn create_project(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Path(organization_id): Path<Uuid>,
    Json(request): Json<CreateProjectRequest>,
) -> Result<ResponseJson<CreateProjectResponse>, StatusCode> {
    tracing::error!("=== CREATE PROJECT FUNCTION CALLED ===");
    info!(
        "Creating project with repository {} and branch {} for user {} in organization {}",
        request.repo_id, request.branch, user.id, organization_id
    );

    // Validate request fields
    if request.provider.is_empty() {
        error!("Provider field is empty");
        return Err(StatusCode::BAD_REQUEST);
    }

    if request.branch.is_empty() {
        error!("Branch field is empty");
        return Err(StatusCode::BAD_REQUEST);
    }

    if request.token.is_empty() {
        error!("Token field is empty");
        return Err(StatusCode::BAD_REQUEST);
    }

    // Check if user is a member of the organization
    let db = establish_connection().await.map_err(|e| {
        error!("Database connection failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let user_in_org = OrganizationUsers::find()
        .filter(organization_users::Column::UserId.eq(user.id))
        .filter(organization_users::Column::OrganizationId.eq(organization_id))
        .one(&db)
        .await
        .map_err(|e| {
            error!("Failed to check organization membership: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if user_in_org.is_none() {
        error!(
            "User {} is not a member of organization {}",
            user.id, organization_id
        );
        return Err(StatusCode::FORBIDDEN);
    }

    info!(
        "User {} is a member of organization {}",
        user.id, organization_id
    );

    let provider = match entity::projects::ProjectProvider::from_str(&request.provider) {
        Ok(provider) => {
            info!("Valid provider: {:?}", provider);
            provider
        }
        Err(e) => {
            error!("Invalid provider '{}': {}", request.provider, e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    match ProjectService::create_project_with_repo_and_pull(
        organization_id,
        request.token,
        request.repo_id,
        request.branch,
        provider,
    )
    .await
    {
        Ok((project, branch, local_path)) => {
            info!(
                "Project '{}' created successfully with ID: {}",
                project.name, project.id
            );

            Ok(ResponseJson(CreateProjectResponse {
                success: true,
                message: "Project created and repository cloned successfully".to_string(),
                project_id: project.id,
                branch_id: Some(branch.id),
                local_path: Some(local_path),
            }))
        }
        Err(e) => {
            error!("Failed to create project: {}", e);
            Ok(ResponseJson(CreateProjectResponse {
                success: false,
                message: format!("Failed to create project: {e}"),
                project_id: Uuid::nil(),
                branch_id: None,
                local_path: None,
            }))
        }
    }
}

pub async fn pull_changes(
    Query(query): Query<BranchQuery>,
    Path(project_id): Path<Uuid>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    match ProjectService::pull_changes(project_id, query.branch.clone()).await {
        Ok(_) => Ok(ResponseJson(ProjectResponse {
            success: true,
            message: "Changes pulled successfully".to_string(),
        })),
        Err(e) => {
            error!("Failed to pull changes: {}", e);
            Ok(ResponseJson(ProjectResponse {
                success: false,
                message: format!("Failed to pull changes: {e}"),
            }))
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PushChangesRequest {
    pub commit_message: Option<String>,
}

pub async fn push_changes(
    Query(query): Query<BranchQuery>,
    Path(project_id): Path<Uuid>,
    Json(request): Json<PushChangesRequest>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    let commit_message = request
        .commit_message
        .clone()
        .unwrap_or_else(|| "Auto-commit: Oxy changes".to_string());

    match ProjectService::push_changes(project_id, query.branch.clone(), commit_message).await {
        Ok(_) => Ok(ResponseJson(ProjectResponse {
            success: true,
            message: "Changes pushed successfully".to_string(),
        })),
        Err(e) => {
            error!("Failed to push changes: {}", e);
            Ok(ResponseJson(ProjectResponse {
                success: false,
                message: format!("Failed to push changes: {e}"),
            }))
        }
    }
}

pub async fn change_git_token(
    Path(project_id): Path<Uuid>,
    Json(request): Json<GitHubTokenQuery>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    match ProjectService::update_project_token(project_id, request.token).await {
        Ok(_) => Ok(ResponseJson(ProjectResponse {
            success: true,
            message: "Git token updated successfully".to_string(),
        })),
        Err(e) => {
            error!("Failed to update git token: {}", e);
            Ok(ResponseJson(ProjectResponse {
                success: false,
                message: format!("Failed to update git token: {e}"),
            }))
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RevisionInfoResponse {
    pub current_revision: Option<String>,
    pub latest_revision: Option<String>,
    pub current_commit: Option<crate::github::CommitInfo>,
    pub latest_commit: Option<crate::github::CommitInfo>,
    pub sync_status: String,
    pub last_sync_time: Option<String>,
}

pub async fn get_revision_info(
    Path(project_id): Path<Uuid>,
    Query(query): Query<BranchQuery>,
) -> Result<ResponseJson<RevisionInfoResponse>, StatusCode> {
    let info = ProjectService::get_revision_info(project_id, query.branch.clone()).await?;

    Ok(ResponseJson(info))
}

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
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path(project_id): Path<Uuid>,
) -> Result<ResponseJson<ProjectDetailsResponse>, StatusCode> {
    info!("Getting project details for ID: {}", project_id);

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
        organization_id: project.organization_id,
        provider: project.provider,
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
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path(project_id): Path<Uuid>,
) -> Result<ResponseJson<ProjectBranchesResponse>, StatusCode> {
    info!("Getting branches for project: {}", project_id);

    match ProjectService::get_project_branches(project_id).await {
        Ok(branches) => {
            info!(
                "Found {} branches for project {}",
                branches.len(),
                project_id
            );
            Ok(ResponseJson(ProjectBranchesResponse { branches }))
        }
        Err(e) => {
            error!("Failed to get branches for project {}: {}", project_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn switch_project_branch(
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path(project_id): Path<Uuid>,
    Json(request): Json<SwitchBranchRequest>,
) -> Result<ResponseJson<ProjectBranch>, StatusCode> {
    info!("Getting switched branches for project: {}", project_id);

    match ProjectService::switch_project_branch(project_id, request.branch).await {
        Ok(branch) => {
            info!("Successfully switched branches for project {}", project_id);
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

pub async fn switch_project_active_branch(
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path(project_id): Path<Uuid>,
    Json(request): Json<SwitchBranchRequest>,
) -> Result<ResponseJson<ProjectBranch>, StatusCode> {
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
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<ResponseJson<ProjectResponse>, StatusCode> {
    info!("Deleting project: {}", project_id);

    let db = establish_connection()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let requester_role = OrganizationUsers::find()
        .filter(organization_users::Column::OrganizationId.eq(organization_id))
        .filter(organization_users::Column::UserId.eq(requester.id))
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListProjectsResponse {
    pub projects: Vec<ProjectSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProjectSummary {
    pub id: Uuid,
    pub name: String,
    pub organization_id: Uuid,
    pub provider: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn get_project_status(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path(ProjectPath { project_id }): Path<ProjectPath>,
) -> Result<axum::response::Json<ProjectStatus>, StatusCode> {
    info!("Getting overall project status for project: {}", project_id);

    let project_path = project_manager.config_manager.project_path();

    let (is_config_valid, required_secrets, error) =
        match ProjectBuilder::new().with_project_path(&project_path).await {
            Ok(builder) => {
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

#[utoipa::path(
    get,
    path = "/organizations/{organization_id}/projects",
    params(
        ("organization_id" = Uuid, Path, description = "Organization ID")
    ),
    responses(
        (status = 200, description = "Projects retrieved successfully", body = ListProjectsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Projects"
)]
pub async fn list_projects(
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    axum::extract::Path(organization_id): axum::extract::Path<Uuid>,
) -> Result<axum::response::Json<ListProjectsResponse>, StatusCode> {
    info!("Listing projects for organization: {}", organization_id);

    match ProjectService::get_projects_by_organization(organization_id).await {
        Ok(projects) => {
            let mut project_summaries = Vec::new();

            for project in projects {
                project_summaries.push(ProjectSummary {
                    id: project.id,
                    name: project.name,
                    organization_id: project.organization_id,
                    provider: project.provider,
                    created_at: project.created_at.to_string(),
                    updated_at: project.updated_at.to_string(),
                });
            }

            Ok(axum::response::Json(ListProjectsResponse {
                projects: project_summaries,
            }))
        }
        Err(e) => {
            error!("Failed to list projects: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Simple health check endpoint to test routing
#[utoipa::path(
    get,
    path = "/organizations/{organization_id}/projects/health",
    params(
        ("organization_id" = Uuid, Path, description = "Organization ID")
    ),
    responses(
        (status = 200, description = "Health check successful", body = String),
    ),
    tag = "Projects"
)]
pub async fn project_health_check(
    Path(organization_id): Path<Uuid>,
) -> Result<ResponseJson<String>, StatusCode> {
    info!("Health check for organization: {}", organization_id);
    Ok(ResponseJson("OK".to_string()))
}
