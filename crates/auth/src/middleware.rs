use axum::{
    extract::State,
    http::{Method, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

use crate::user::UserService;

use crate::{
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
    pub fn built_in(cloud: bool) -> Self {
        Self {
            authenticator: Arc::new(BuiltInAuthenticator::new(cloud)),
        }
    }
}

pub async fn auth_middleware<T: Authenticator>(
    State(auth_state): State<AuthState<T>>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Allow OPTIONS requests (CORS preflight) to pass through without authentication
    if request.method() == Method::OPTIONS {
        return Ok(next.run(request).await);
    }

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

/// Middleware for the internal port that auto-authenticates as an internal user.
/// Uses UserService::get_or_create_user to ensure the user exists in the database,
/// so that foreign key constraints in downstream handlers work correctly.
pub async fn internal_auth_middleware(
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if request.method() == Method::OPTIONS {
        return Ok(next.run(request).await);
    }

    let internal_identity = crate::types::Identity {
        idp_id: Some("internal-user".to_string()),
        email: "internal@localhost".to_string(),
        name: Some("Internal".to_string()),
        picture: None,
    };

    let user = UserService::get_or_create_user(&internal_identity)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get or create internal user: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

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
