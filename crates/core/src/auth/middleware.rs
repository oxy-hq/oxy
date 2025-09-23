use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

use crate::auth::user::UserService;

use super::{
    authenticator::Authenticator, built_in::BuiltInAuthenticator, types::AuthenticatedUser,
};
use entity::users::{UserRole, UserStatus};

pub struct AuthState<T> {
    authenticator: Arc<T>,
}

impl<T> Clone for AuthState<T> {
    fn clone(&self) -> Self {
        Self {
            authenticator: Arc::clone(&self.authenticator),
        }
    }
}

impl AuthState<BuiltInAuthenticator> {
    pub fn built_in() -> Self {
        Self {
            authenticator: Arc::new(BuiltInAuthenticator::new()),
        }
    }
}

pub async fn auth_middleware<T: Authenticator>(
    State(auth_state): State<AuthState<T>>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let headers = request.headers();

    // Authenticate using the configured authenticator
    let claims = auth_state
        .authenticator
        .authenticate(headers)
        .await
        .map_err(|err| {
            tracing::error!("Authentication failed: {}", err);
            err.into()
        })?;

    // Find or create user based on claims
    let user = UserService::get_or_create_user(&claims)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find or create user: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Check if user is active
    if user.status != UserStatus::Active {
        tracing::warn!(
            "Inactive user {} (status: {}) attempted to access protected route",
            user.email,
            user.status.as_str()
        );
        return Err(StatusCode::FORBIDDEN);
    }

    // Add user to request extensions for downstream handlers
    request.extensions_mut().insert(user);

    Ok(next.run(request).await)
}

pub async fn admin_middleware(
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let user = request
        .extensions()
        .get::<AuthenticatedUser>()
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if user.role != UserRole::Admin {
        tracing::warn!(
            "Non-admin user {} attempted to access admin route",
            user.email
        );
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(next.run(request).await)
}
