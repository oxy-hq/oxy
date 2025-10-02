use crate::auth::extractor::AuthenticatedUserExtractor;
use crate::db::client::establish_connection;
use axum::{extract::Json as JsonExtractor, http::StatusCode, response::Json};
use entity::{
    organization_users, organizations,
    prelude::{OrganizationUsers, Organizations},
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Serialize, ToSchema)]
pub struct OrganizationResponse {
    pub id: String,
    pub name: String,
    pub role: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, ToSchema)]
pub struct OrganizationListResponse {
    pub organizations: Vec<OrganizationResponse>,
    pub total: usize,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateOrganizationRequest {
    pub name: String,
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
pub struct AddUserToOrgRequest {
    pub email: String,
    pub role: String,
}

#[derive(Deserialize)]
pub struct UpdateUserRoleRequest {
    pub user_id: Uuid,
    pub role: String,
}

#[derive(Deserialize)]
pub struct RemoveUserFromOrgRequest {
    pub user_id: Uuid,
}

pub async fn list_users(
    _user: AuthenticatedUserExtractor,
    axum::extract::Path(organization_id): axum::extract::Path<Uuid>,
) -> Result<Json<UserListResponse>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("Database connection failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let org_users = OrganizationUsers::find()
        .filter(organization_users::Column::OrganizationId.eq(organization_id))
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query organization users: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let user_ids: Vec<Uuid> = org_users.iter().map(|ou| ou.user_id).collect();

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
            org_users
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

/// Create a new organization
///
/// Creates a new organization with the authenticated user as the owner. Organizations
/// allow teams to collaborate on projects and manage access control. The creator is
/// automatically assigned the owner role.
#[utoipa::path(
    post,
    path = "/organizations",
    request_body = CreateOrganizationRequest,
    responses(
        (status = 201, description = "Organization created successfully", body = OrganizationResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    )
)]
pub async fn create_organization(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    JsonExtractor(req): JsonExtractor<CreateOrganizationRequest>,
) -> Result<Json<OrganizationResponse>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("Database connection failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let organization_id = Uuid::new_v4();
    let now = chrono::Utc::now().into();

    let organization = organizations::ActiveModel {
        id: Set(organization_id),
        name: Set(req.name.clone()),
        created_at: Set(now),
        updated_at: Set(now),
    };

    let created_org = organization.insert(&db).await.map_err(|e| {
        tracing::error!("Failed to create organization: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let org_user = organization_users::ActiveModel {
        id: Set(Uuid::new_v4()),
        organization_id: Set(organization_id),
        user_id: Set(user.id),
        role: Set("owner".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    };

    org_user.insert(&db).await.map_err(|e| {
        tracing::error!("Failed to add user to organization: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(OrganizationResponse {
        id: created_org.id.to_string(),
        name: created_org.name,
        role: "owner".to_string(),
        created_at: created_org.created_at.to_string(),
        updated_at: created_org.updated_at.to_string(),
    }))
}

#[utoipa::path(
    get,
    path = "/organizations",
    responses(
        (status = 200, description = "Success", body = OrganizationListResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    )
)]
pub async fn list_organizations(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<Json<OrganizationListResponse>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("Database connection failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let user_org_roles = OrganizationUsers::find()
        .filter(organization_users::Column::UserId.eq(user.id))
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query user organizations: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let org_ids: Vec<Uuid> = user_org_roles.iter().map(|ur| ur.organization_id).collect();

    let orgs = Organizations::find()
        .filter(organizations::Column::Id.is_in(org_ids))
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query organizations: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut organizations = Vec::new();
    for org in orgs {
        if let Some(user_role) = user_org_roles
            .iter()
            .find(|ur| ur.organization_id == org.id)
        {
            organizations.push(OrganizationResponse {
                id: org.id.to_string(),
                name: org.name,
                role: user_role.role.clone(),
                created_at: org.created_at.to_string(),
                updated_at: org.updated_at.to_string(),
            });
        }
    }

    let total = organizations.len();

    Ok(Json(OrganizationListResponse {
        organizations,
        total,
    }))
}

/// List all organizations for the authenticated user
///
/// Retrieves all organizations where the user is a member, along with their role
/// (owner, admin, member). Returns organization metadata including names, roles,
/// and timestamps.
pub async fn add_user_to_organization(
    AuthenticatedUserExtractor(requester): AuthenticatedUserExtractor,
    axum::extract::Path(organization_id): axum::extract::Path<Uuid>,
    JsonExtractor(req): JsonExtractor<AddUserToOrgRequest>,
) -> Result<StatusCode, StatusCode> {
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
    let user = entity::users::Entity::find()
        .filter(entity::users::Column::Email.eq(req.email.clone()))
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let user = match user {
        Some(u) => u,
        None => return Err(StatusCode::NOT_FOUND),
    };

    let already_in_org = OrganizationUsers::find()
        .filter(organization_users::Column::OrganizationId.eq(organization_id))
        .filter(organization_users::Column::UserId.eq(user.id))
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .is_some();
    if already_in_org {
        return Err(StatusCode::CONFLICT);
    }
    let now = chrono::Utc::now().into();
    let org_user = organization_users::ActiveModel {
        id: Set(Uuid::new_v4()),
        organization_id: Set(organization_id),
        user_id: Set(user.id),
        role: Set(req.role),
        created_at: Set(now),
        updated_at: Set(now),
    };
    org_user
        .insert(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::CREATED)
}

pub async fn update_user_role_in_organization(
    AuthenticatedUserExtractor(requester): AuthenticatedUserExtractor,
    axum::extract::Path(organization_id): axum::extract::Path<Uuid>,
    JsonExtractor(req): JsonExtractor<UpdateUserRoleRequest>,
) -> Result<StatusCode, StatusCode> {
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
    let org_user = OrganizationUsers::find()
        .filter(organization_users::Column::OrganizationId.eq(organization_id))
        .filter(organization_users::Column::UserId.eq(req.user_id))
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(mut org_user) = org_user {
        org_user.role = req.role;
        let mut active_model: organization_users::ActiveModel = org_user.into();
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

pub async fn remove_user_from_organization(
    AuthenticatedUserExtractor(requester): AuthenticatedUserExtractor,
    axum::extract::Path((organization_id, user_id)): axum::extract::Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
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
    let res = OrganizationUsers::delete_many()
        .filter(organization_users::Column::OrganizationId.eq(organization_id))
        .filter(organization_users::Column::UserId.eq(user_id))
        .exec(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if res.rows_affected > 0 {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
