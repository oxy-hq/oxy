use crate::server::service::project::ProjectService;
use crate::server::service::project::models::CreateWorkspaceRequest;
use axum::{extract::Json as JsonExtractor, http::StatusCode, response::Json};
use entity::{
    prelude::{WorkspaceUsers, Workspaces},
    workspace_users, workspaces,
};
use oxy::api_types::{ProjectInfo, WorkspaceResponse};
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

// WorkspaceResponse and ProjectInfo imported from oxy::api_types

#[derive(Serialize, ToSchema)]
pub struct WorkspaceListResponse {
    pub workspaces: Vec<WorkspaceResponse>,
    pub total: usize,
}

#[derive(Serialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub name: String,
    pub picture: Option<String>,
    pub role: String,
}

#[derive(Serialize)]
pub struct UserListResponse {
    pub users: Vec<UserInfo>,
    pub total: usize,
}

#[derive(Deserialize)]
pub struct AddUserToWorkspaceRequest {
    pub email: String,
    pub role: String,
}

#[derive(Deserialize)]
pub struct UpdateUserRoleRequest {
    pub user_id: Uuid,
    pub role: String,
}

#[derive(Deserialize)]
pub struct RemoveUserFromWorkspaceRequest {
    pub user_id: Uuid,
}

pub async fn list_users(
    _user: AuthenticatedUserExtractor,
    axum::extract::Path(workspace_id): axum::extract::Path<Uuid>,
) -> Result<Json<UserListResponse>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("Database connection failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let workspace_users = WorkspaceUsers::find()
        .filter(workspace_users::Column::WorkspaceId.eq(workspace_id))
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query workspace users: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let user_ids: Vec<Uuid> = workspace_users.iter().map(|ou| ou.user_id).collect();

    let users = entity::users::Entity::find()
        .filter(entity::users::Column::Id.is_in(user_ids.clone()))
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query users: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let user_infos: Vec<UserInfo> = users
        .into_iter()
        .filter_map(|user| {
            workspace_users
                .iter()
                .find(|ou| ou.user_id == user.id)
                .map(|ou| UserInfo {
                    id: user.id.to_string(),
                    email: user.email.clone(),
                    name: user.name.clone(),
                    picture: user.picture.clone(),
                    role: ou.role.clone(),
                })
        })
        .collect();
    let total = user_infos.len();

    Ok(Json(UserListResponse {
        users: user_infos,
        total,
    }))
}

/// Create a new workspace
///
/// Creates a new workspace for the authenticated user with owner role. Optionally creates
/// an associated project by cloning from a Git repository (GitHub) when repo_id, token,
/// branch, and provider are provided. Returns workspace details including the created project.
#[utoipa::path(
    post,
    path = "/workspaces",
    request_body = CreateWorkspaceRequest,
    responses(
        (status = 201, description = "Workspace created successfully", body = WorkspaceResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
       ("ApiKey" = [])
    )
)]
pub async fn create_workspace(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    JsonExtractor(req): JsonExtractor<CreateWorkspaceRequest>,
) -> Result<Json<WorkspaceResponse>, StatusCode> {
    ProjectService::create_workspace_new(user.id, req).await
}

/// List all workspaces for the authenticated user
///
/// Retrieves all workspaces where the user is a member, along with their role (owner, admin, member)
/// and associated project information. Returns workspace metadata including creation timestamps
/// and the first project in each workspace.
#[utoipa::path(
    get,
    path = "/workspaces",
    responses(
        (status = 200, description = "Success", body = WorkspaceListResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    )
)]
pub async fn list_workspaces(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<Json<WorkspaceListResponse>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("Database connection failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let user_ws_roles = WorkspaceUsers::find()
        .filter(workspace_users::Column::UserId.eq(user.id))
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query user workspaces: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let ws_ids: Vec<Uuid> = user_ws_roles.iter().map(|ur| ur.workspace_id).collect();

    let wss = Workspaces::find()
        .filter(workspaces::Column::Id.is_in(ws_ids))
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query workspaces: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut workspaces = Vec::new();
    for ws in wss {
        if let Some(user_role) = user_ws_roles.iter().find(|ur| ur.workspace_id == ws.id) {
            let projects = match ProjectService::get_projects_by_workspace(ws.id).await {
                Ok(projects) => projects,
                Err(e) => {
                    tracing::error!("Failed to get projects for workspace {}: {}", ws.id, e);
                    vec![]
                }
            };

            let project = if !projects.is_empty() {
                let project = &projects[0];
                Some(ProjectInfo {
                    id: project.id,
                    name: project.name.clone(),
                    workspace_id: project.workspace_id,
                    created_at: project.created_at.to_string(),
                    updated_at: project.updated_at.to_string(),
                })
            } else {
                None
            };

            workspaces.push(WorkspaceResponse {
                id: ws.id,
                name: ws.name,
                role: Some(user_role.role.clone()),
                created_at: ws.created_at.to_string(),
                updated_at: ws.updated_at.to_string(),
                project,
            });
        }
    }

    let total = workspaces.len();

    Ok(Json(WorkspaceListResponse { workspaces, total }))
}

pub async fn add_user_to_workspace(
    AuthenticatedUserExtractor(requester): AuthenticatedUserExtractor,
    axum::extract::Path(workspace_id): axum::extract::Path<Uuid>,
    JsonExtractor(req): JsonExtractor<AddUserToWorkspaceRequest>,
) -> Result<StatusCode, StatusCode> {
    let db = establish_connection()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let requester_role = WorkspaceUsers::find()
        .filter(workspace_users::Column::WorkspaceId.eq(workspace_id))
        .filter(workspace_users::Column::UserId.eq(requester.id))
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(|wu| wu.role);
    if requester_role.as_deref() != Some("owner") && requester_role.as_deref() != Some("admin") {
        return Err(StatusCode::FORBIDDEN);
    }
    let user = entity::users::Entity::find()
        .filter(entity::users::Column::Email.eq(req.email.clone()))
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let user = match user {
        Some(u) => u,
        None => return Err(StatusCode::NOT_FOUND),
    };

    let already_in_ws = WorkspaceUsers::find()
        .filter(workspace_users::Column::WorkspaceId.eq(workspace_id))
        .filter(workspace_users::Column::UserId.eq(user.id))
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .is_some();
    if already_in_ws {
        return Err(StatusCode::CONFLICT);
    }
    let now = chrono::Utc::now().into();
    let ws_user = workspace_users::ActiveModel {
        id: Set(Uuid::new_v4()),
        workspace_id: Set(workspace_id),
        user_id: Set(user.id),
        role: Set(req.role),
        created_at: Set(now),
        updated_at: Set(now),
    };
    ws_user
        .insert(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::CREATED)
}

pub async fn update_user_role_in_workspace(
    AuthenticatedUserExtractor(requester): AuthenticatedUserExtractor,
    axum::extract::Path(workspace_id): axum::extract::Path<Uuid>,
    JsonExtractor(req): JsonExtractor<UpdateUserRoleRequest>,
) -> Result<StatusCode, StatusCode> {
    let db = establish_connection()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let requester_role = WorkspaceUsers::find()
        .filter(workspace_users::Column::WorkspaceId.eq(workspace_id))
        .filter(workspace_users::Column::UserId.eq(requester.id))
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(|wu| wu.role);
    if requester_role.as_deref() != Some("owner") && requester_role.as_deref() != Some("admin") {
        return Err(StatusCode::FORBIDDEN);
    }
    let ws_user = WorkspaceUsers::find()
        .filter(workspace_users::Column::WorkspaceId.eq(workspace_id))
        .filter(workspace_users::Column::UserId.eq(req.user_id))
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(mut ws_user) = ws_user {
        ws_user.role = req.role;
        let mut active_model: workspace_users::ActiveModel = ws_user.into();
        active_model.updated_at = Set(chrono::Utc::now().into());
        active_model
            .update(&db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(StatusCode::OK)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

pub async fn remove_user_from_workspace(
    AuthenticatedUserExtractor(requester): AuthenticatedUserExtractor,
    axum::extract::Path((workspace_id, user_id)): axum::extract::Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let db = establish_connection()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let requester_role = WorkspaceUsers::find()
        .filter(workspace_users::Column::WorkspaceId.eq(workspace_id))
        .filter(workspace_users::Column::UserId.eq(requester.id))
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(|wu| wu.role);
    if requester_role.as_deref() != Some("owner") && requester_role.as_deref() != Some("admin") {
        return Err(StatusCode::FORBIDDEN);
    }
    let res = WorkspaceUsers::delete_many()
        .filter(workspace_users::Column::WorkspaceId.eq(workspace_id))
        .filter(workspace_users::Column::UserId.eq(user_id))
        .exec(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if res.rows_affected > 0 {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
