use axum::{
    http::{HeaderMap, StatusCode, header::SET_COOKIE},
    response::Json,
};
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_auth::types::AuthenticatedUser;
use oxy_auth::user::UserService;
use serde::Serialize;

use crate::server::api::auth::clear_session_cookie;

/// Global profile fields returned by `GET /user`. Role and admin status are
/// per-org, so they are intentionally omitted here — read them from
/// `OrgInfo` in the login response or from `GET /orgs`. Workspace-scoped
/// routes receive the resolved role via the `EffectiveWorkspaceRole`
/// extractor. `is_owner` mirrors the `OXY_OWNER` allow-list and lets the
/// frontend route Oxy staff to the admin shell. `is_app_admin` reflects
/// membership in the `app_admins` table and gates the customer-apps
/// surface.
#[derive(Serialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub name: String,
    pub picture: Option<String>,
    pub status: String,
    pub is_owner: bool,
    pub is_app_admin: bool,
}

#[derive(Serialize)]
pub struct LogoutResponse {
    pub logout_url: Option<String>,
    pub success: bool,
    pub message: String,
}

/// Build a [`UserInfo`] from the authenticated user. Async because
/// `is_app_admin` is now a DB-backed check (see `app_admins` table).
pub async fn user_info_from(user: AuthenticatedUser) -> UserInfo {
    let is_owner = crate::server::api::middlewares::oxy_owner_guard::is_oxy_owner(&user.email);
    let is_app_admin =
        crate::server::api::middlewares::oxy_app_admin_guard::is_oxy_app_admin(&user.email).await;
    UserInfo {
        id: user.id.to_string(),
        email: user.email,
        name: user.name,
        picture: user.picture,
        status: user.status.as_str().to_string(),
        is_owner,
        is_app_admin,
    }
}

pub async fn logout() -> Result<(HeaderMap, Json<LogoutResponse>), StatusCode> {
    let mut headers = HeaderMap::new();
    if let Ok(value) = clear_session_cookie().parse() {
        headers.insert(SET_COOKIE, value);
    }
    Ok((
        headers,
        Json(LogoutResponse {
            logout_url: None,
            success: true,
            message: "Built-in logout successful".to_string(),
        }),
    ))
}

pub async fn get_current_user(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<Json<UserInfo>, StatusCode> {
    Ok(Json(user_info_from(user).await))
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
            // Look up existing user only — do not auto-create. Closes #16.
            // User rows are created by the auth/sign-up flow, not by a public GET.
            match UserService::find_user_by_identity(&identity).await {
                Ok(Some(user)) => Ok(Json(Some(user_info_from(user).await))),
                Ok(None) => Ok(Json(None)),
                Err(e) => {
                    tracing::error!("Failed to lookup user from identity: {}", e);
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
