use crate::auth::extractor::AuthenticatedUserExtractor;
use crate::auth::types::{AuthMode, AuthenticatedUser};
use crate::auth::user::UserService;
use axum::{
    extract::{Json as JsonExtractor, Path, State},
    http::StatusCode,
    response::Json,
};
use entity::users::{UserRole, UserStatus};
use sea_orm::EntityTrait;
use serde::{Deserialize, Serialize};
use std::env;
use url::Url;
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

pub async fn logout(State(auth_mode): State<AuthMode>) -> Result<Json<LogoutResponse>, StatusCode> {
    match auth_mode {
        AuthMode::Cognito => {
            // Handle Cognito logout by returning the logout URL
            let user_pool_id =
                env::var("AWS_COGNITO_USER_POOL_ID").map_err(|_| StatusCode::NOT_FOUND)?;
            let region =
                env::var("AWS_COGNITO_REGION").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let client_id =
                env::var("AWS_COGNITO_CLIENT_ID").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let mut logout_url = Url::parse(&format!(
                "https://{user_pool_id}.auth.{region}.amazoncognito.com/logout"
            ))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            logout_url
                .query_pairs_mut()
                .append_pair("client_id", &client_id);

            Ok(Json(LogoutResponse {
                logout_url: Some(logout_url.to_string()),
                success: true,
                message: "Cognito logout URL generated successfully".to_string(),
            }))
        }
        AuthMode::IAP | AuthMode::IAPCloudRun => {
            // For IAP, there's no specific logout URL needed as it's handled by Google
            Ok(Json(LogoutResponse {
                logout_url: None,
                success: true,
                message: "IAP logout handled by identity provider".to_string(),
            }))
        }
        AuthMode::BuiltIn => {
            // For built-in auth, just indicate successful logout
            Ok(Json(LogoutResponse {
                logout_url: None,
                success: true,
                message: "Build-in logout successful".to_string(),
            }))
        }
    }
}

pub async fn get_current_user(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<Json<UserInfo>, StatusCode> {
    Ok(Json(user.into()))
}
