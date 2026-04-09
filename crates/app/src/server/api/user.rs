use axum::{
    extract::{Json as JsonExtractor, Path},
    http::StatusCode,
    response::Json,
};
use entity::users::{UserRole, UserStatus};
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_auth::types::AuthenticatedUser;
use oxy_auth::user::UserService;
use sea_orm::EntityTrait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize)]
pub struct UserListResponse {
    pub users: Vec<UserInfo>,
    pub total: usize,
}

#[derive(Serialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub name: String,
    pub picture: Option<String>,
    pub role: String,
    pub status: String,
    /// True when the user has admin access. In cloud mode this reflects the DB role.
    /// In local mode it is also true when the user's email is in `config.admins`
    /// (or the list is empty — permissive default).
    pub is_admin: bool,
}

#[derive(Serialize)]
pub struct LogoutResponse {
    pub logout_url: Option<String>,
    pub success: bool,
    pub message: String,
}

#[derive(Serialize)]
pub struct MessageResponse {
    pub message: String,
}

#[derive(Deserialize)]
pub struct UpdateUserRequest {
    pub status: Option<String>,
    pub role: Option<String>,
}

#[derive(Deserialize)]
pub struct BatchUsersRequest {
    pub user_ids: Vec<String>,
}

impl From<AuthenticatedUser> for UserInfo {
    fn from(user: AuthenticatedUser) -> Self {
        let is_admin = user.role.is_admin_or_above();
        Self {
            id: user.id.to_string(),
            email: user.email,
            name: user.name,
            picture: user.picture,
            role: user.role.as_str().to_string(),
            status: user.status.as_str().to_string(),
            is_admin,
        }
    }
}

pub async fn list_users(
    _user: AuthenticatedUserExtractor,
) -> Result<Json<UserListResponse>, StatusCode> {
    let users = UserService::list_all_users().await.map_err(|e| {
        tracing::error!("Failed to list users: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let user_infos: Vec<UserInfo> = users.into_iter().map(|user| user.into()).collect();
    let total = user_infos.len();

    Ok(Json(UserListResponse {
        users: user_infos,
        total,
    }))
}

pub async fn batch_get_users(
    _user: AuthenticatedUserExtractor,
    JsonExtractor(payload): JsonExtractor<BatchUsersRequest>,
) -> Result<Json<UserListResponse>, StatusCode> {
    use entity::prelude::Users;
    use sea_orm::ColumnTrait;
    use sea_orm::EntityTrait;
    use sea_orm::QueryFilter;

    let connection = oxy::database::client::establish_connection()
        .await
        .map_err(|e| {
            tracing::error!("Failed to establish database connection: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Parse UUIDs from strings
    let user_uuids: Result<Vec<Uuid>, _> = payload
        .user_ids
        .iter()
        .map(|id| Uuid::parse_str(id))
        .collect();

    let user_uuids = user_uuids.map_err(|_| StatusCode::BAD_REQUEST)?;

    let users = Users::find()
        .filter(entity::users::Column::Id.is_in(user_uuids))
        .all(&connection)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query users: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let user_infos: Vec<UserInfo> = users
        .into_iter()
        .map(|user| {
            let auth_user: AuthenticatedUser = user.into();
            auth_user.into()
        })
        .collect();

    let total = user_infos.len();

    Ok(Json(UserListResponse {
        users: user_infos,
        total,
    }))
}

pub async fn delete_user(
    AuthenticatedUserExtractor(current_user): AuthenticatedUserExtractor,
    Path(user_id): Path<String>,
) -> Result<Json<MessageResponse>, StatusCode> {
    let user_uuid = Uuid::parse_str(&user_id).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Prevent users from deleting themselves
    if current_user.id == user_uuid {
        tracing::warn!(
            "User {} attempted to delete their own account",
            current_user.email
        );
        return Err(StatusCode::FORBIDDEN);
    }

    UserService::delete_user(user_uuid).await.map_err(|e| {
        tracing::error!("Failed to delete user: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(MessageResponse {
        message: "User deleted successfully".to_string(),
    }))
}

pub async fn update_user(
    AuthenticatedUserExtractor(current_user): AuthenticatedUserExtractor,
    Path(user_id): Path<String>,
    JsonExtractor(payload): JsonExtractor<UpdateUserRequest>,
) -> Result<Json<MessageResponse>, StatusCode> {
    let user_uuid = Uuid::parse_str(&user_id).map_err(|_| StatusCode::BAD_REQUEST)?;

    // ── Role change ─────────────────────────────────────────────────────────
    if let Some(ref role_str) = payload.role {
        let new_role = UserRole::from_str(role_str).map_err(|_| StatusCode::BAD_REQUEST)?;

        // Owner role cannot be granted via API — it is only set by bootstrap.
        if new_role == UserRole::Owner {
            return Err(StatusCode::FORBIDDEN);
        }

        // Cannot change own role.
        if current_user.id == user_uuid {
            return Err(StatusCode::FORBIDDEN);
        }

        let connection = establish_connection().await.map_err(|e| {
            tracing::error!("DB connection failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        let target_user = entity::prelude::Users::find_by_id(user_uuid)
            .one(&connection)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get target user: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or(StatusCode::NOT_FOUND)?;

        // Owner's role cannot be changed by anyone.
        if target_user.role == UserRole::Owner {
            return Err(StatusCode::FORBIDDEN);
        }

        // Permission rules:
        //   Owner → can promote to Admin or demote to Member.
        //   Admin → can only demote to Member (cannot grant Admin).
        //   Member → no permission.
        match current_user.role {
            // Owner may promote to Admin or demote to Member — no role is off-limits
            // for an Owner (except changing another Owner, which is blocked above).
            UserRole::Owner => {}
            UserRole::Admin => {
                if new_role != UserRole::Member {
                    tracing::warn!(
                        "Admin {} attempted to grant role {:?} (requires Owner)",
                        current_user.email,
                        new_role
                    );
                    return Err(StatusCode::FORBIDDEN);
                }
            }
            UserRole::Member => {
                return Err(StatusCode::FORBIDDEN);
            }
        }

        UserService::update_user_role(user_uuid, new_role)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update user role: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    }

    // ── Status change ────────────────────────────────────────────────────────
    if let Some(ref status_str) = payload.status {
        if ![UserStatus::Active.as_str(), UserStatus::Deleted.as_str()]
            .contains(&status_str.as_str())
        {
            return Err(StatusCode::BAD_REQUEST);
        }

        let status = UserStatus::from_str(status_str).map_err(|_| StatusCode::BAD_REQUEST)?;

        // Prevent users from setting their own status to "deleted".
        if current_user.id == user_uuid && status == UserStatus::Deleted {
            tracing::warn!(
                "User {} attempted to delete their own account via status update",
                current_user.email
            );
            return Err(StatusCode::FORBIDDEN);
        }

        // Prevent admins and owners from being deactivated.
        if status == UserStatus::Deleted && current_user.id != user_uuid {
            let connection = establish_connection().await.map_err(|e| {
                tracing::error!("DB connection failed: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            let target_user = entity::prelude::Users::find_by_id(user_uuid)
                .one(&connection)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to get target user: {}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;

            if let Some(target_user) = target_user
                && target_user.role.is_admin_or_above()
            {
                tracing::warn!(
                    "User {} attempted to deactivate privileged user {}",
                    current_user.email,
                    target_user.email
                );
                return Err(StatusCode::FORBIDDEN);
            }
        }

        UserService::update_user_status(user_uuid, status)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update user status: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    }

    Ok(Json(MessageResponse {
        message: "User updated successfully".to_string(),
    }))
}

pub async fn logout() -> Result<Json<LogoutResponse>, StatusCode> {
    Ok(Json(LogoutResponse {
        logout_url: None,
        success: true,
        message: "Built-in logout successful".to_string(),
    }))
}

pub async fn get_current_user(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<Json<UserInfo>, StatusCode> {
    Ok(Json(user.into()))
}

/// Public endpoint that returns current user if authenticated, null if not
/// This prevents redirect loops when auth is enabled
pub async fn get_current_user_public(
    axum::extract::State(_app_state): axum::extract::State<crate::server::router::AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Option<UserInfo>>, StatusCode> {
    // Try to authenticate using the same logic as the middleware
    // If successful, return user info; if not, return null (not an error)
    use oxy_auth::authenticator::Authenticator;
    use oxy_auth::built_in::BuiltInAuthenticator;

    let authenticator = BuiltInAuthenticator::new();

    match authenticator.authenticate(&headers).await {
        Ok(identity) => {
            // Get or create user based on identity
            match UserService::get_or_create_user(&identity).await {
                Ok(user) => {
                    let mut user_info = UserInfo::from(user);
                    // DB role is authoritative: if the user already has Admin in the DB
                    // (e.g. first-user bootstrap), keep it. config.admins can only
                    // grant admin to additional users — it cannot revoke it.
                    if !user_info.is_admin {
                        user_info.is_admin = oxy_auth::is_local_admin_from_env(&user_info.email);
                    }
                    Ok(Json(Some(user_info)))
                }
                Err(e) => {
                    tracing::error!("Failed to get user from identity: {}", e);
                    Ok(Json(None))
                }
            }
        }
        Err(_) => {
            // Not authenticated - return null instead of error
            Ok(Json(None))
        }
    }
}
