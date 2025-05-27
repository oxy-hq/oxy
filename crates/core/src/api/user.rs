use crate::auth::extractor::AuthenticatedUserExtractor;
use axum::{extract, http::StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct UserResponse {
    pub id: String,
    pub email: String,
    pub name: String,
    pub picture: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateUserRequest {
    pub name: Option<String>,
    pub picture: Option<String>,
}

/// Get current authenticated user information
pub async fn get_current_user(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<extract::Json<UserResponse>, StatusCode> {
    let user_response = UserResponse {
        id: user.id.to_string(),
        email: user.email,
        name: user.name,
        picture: user.picture,
    };
    Ok(extract::Json(user_response))
}

/// Update current user profile
pub async fn update_current_user(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    extract::Json(update_request): extract::Json<UpdateUserRequest>,
) -> Result<extract::Json<UserResponse>, StatusCode> {
    use crate::auth::user::UserService;

    let updated_user =
        UserService::update_user_profile(user.id, update_request.name, update_request.picture)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update user profile: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let user_response = UserResponse {
        id: updated_user.id.to_string(),
        email: updated_user.email,
        name: updated_user.name,
        picture: updated_user.picture,
    };
    Ok(extract::Json(user_response))
}
