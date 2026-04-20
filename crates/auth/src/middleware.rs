use axum::{
    extract::State,
    http::{Method, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

use crate::user::UserService;

use crate::{authenticator::Authenticator, built_in::BuiltInAuthenticator};
use entity::users::UserStatus;

pub struct AuthState<T> {
    authenticator: Arc<T>,
    /// When true, auth_middleware short-circuits: it never calls the
    /// authenticator and injects the local guest user into extensions.
    /// Set by `AuthState::guest_only()`. Used only by the local-mode router.
    pub guest_only: bool,
}

impl<T> Clone for AuthState<T> {
    fn clone(&self) -> Self {
        Self {
            authenticator: Arc::clone(&self.authenticator),
            guest_only: self.guest_only,
        }
    }
}

impl AuthState<BuiltInAuthenticator> {
    pub fn built_in() -> Self {
        Self {
            authenticator: Arc::new(BuiltInAuthenticator::new()),
            guest_only: false,
        }
    }

    /// Returns an `AuthState` that bypasses the authenticator entirely and
    /// always attaches the local guest user. Only appropriate for the
    /// local-mode router.
    pub fn guest_only() -> Self {
        Self {
            authenticator: Arc::new(BuiltInAuthenticator::new()),
            guest_only: true,
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

    // Guest-only mode (local server): never consult the authenticator, always
    // attach the local guest user. Reuses the existing LOCAL_GUEST_EMAIL
    // mechanism rather than inventing a new sentinel.
    if auth_state.guest_only {
        let identity = crate::types::Identity {
            email: crate::user::LOCAL_GUEST_EMAIL.to_string(),
            name: Some("Local User".to_string()),
            picture: None,
        };
        let user = UserService::get_or_create_user(&identity)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get or create local guest user: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if user.status != UserStatus::Active {
            tracing::warn!(
                "Local guest user is not active (status: {}) — refusing request",
                user.status.as_str()
            );
            return Err(StatusCode::FORBIDDEN);
        }
        request.extensions_mut().insert(user);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_state_is_not_guest_only() {
        let state = AuthState::<BuiltInAuthenticator>::built_in();
        assert!(!state.guest_only);
    }

    #[test]
    fn guest_only_state_flags_itself() {
        let state = AuthState::<BuiltInAuthenticator>::guest_only();
        assert!(state.guest_only);
    }
}
