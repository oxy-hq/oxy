use crate::auth::extractor::AuthenticatedUserExtractor;
use crate::auth::types::AuthenticatedUser;
use crate::auth::user::UserService;
use axum::{
    extract::{Json as JsonExtractor, Path},
    http::StatusCode,
    response::Json,
};
use entity::users::{UserRole, UserStatus};
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
        Self {
            id: user.id.to_string(),
            email: user.email,
            name: user.name,
            picture: user.picture,
            role: user.role.as_str().to_string(),
            status: user.status.as_str().to_string(),
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

    let connection = crate::db::client::establish_connection()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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

    // Validate status if provided
    if let Some(ref status_str) = payload.status {
        if ![UserStatus::Active.as_str(), UserStatus::Deleted.as_str()]
            .contains(&status_str.as_str())
        {
            return Err(StatusCode::BAD_REQUEST);
        }

        let status = UserStatus::from_str(status_str).map_err(|_| StatusCode::BAD_REQUEST)?;

        // Prevent users from setting their own status to "deleted"
        if current_user.id == user_uuid && status == UserStatus::Deleted {
            tracing::warn!(
                "User {} attempted to delete their own account via status update",
                current_user.email
            );
            return Err(StatusCode::FORBIDDEN);
        }

        // Prevent admins from being deactivated
        if status == UserStatus::Deleted && current_user.id != user_uuid {
            // Get the target user to check their role
            let connection = crate::db::client::establish_connection().await?;
            let target_user = entity::prelude::Users::find_by_id(user_uuid)
                .one(&connection)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to get target user: {}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;

            if let Some(target_user) = target_user
                && target_user.role == UserRole::Admin
            {
                tracing::warn!(
                    "User {} attempted to deactivate admin {}",
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
    axum::extract::State(app_state): axum::extract::State<crate::api::router::AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Option<UserInfo>>, StatusCode> {
    // Try to authenticate using the same logic as the middleware
    // If successful, return user info; if not, return null (not an error)
    use crate::auth::authenticator::Authenticator;
    use crate::auth::built_in::BuiltInAuthenticator;

    let authenticator = BuiltInAuthenticator::new(app_state.cloud);

    match authenticator.authenticate(&headers).await {
        Ok(identity) => {
            // Get or create user based on identity
            match UserService::get_or_create_user(&identity).await {
                Ok(user) => Ok(Json(Some(user.into()))),
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
