use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::env;
use std::sync::Arc;

use crate::{
    auth::{cognito::CognitoAuthenticator, iap::IAPAuthenticator, user::UserService},
    config::{auth::Authentication, constants::GCP_IAP_AUD_ENV_VAR},
    errors::OxyError,
};

use super::{authenticator::Authenticator, built_in::BuiltInAuthenticator};

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

impl AuthState<IAPAuthenticator> {
    pub fn iap() -> Result<Self, OxyError> {
        let audience = env::var(GCP_IAP_AUD_ENV_VAR).map_err(|err| {
            OxyError::ConfigurationError(format!(
                "Failed to read {} environment variable: {}",
                GCP_IAP_AUD_ENV_VAR, err
            ))
        })?;

        Ok(Self {
            authenticator: Arc::new(IAPAuthenticator::new(audience, false)),
        })
    }

    pub fn iap_cloud_run() -> Self {
        Self {
            authenticator: Arc::new(IAPAuthenticator::new("".to_string(), true)),
        }
    }
}

impl AuthState<CognitoAuthenticator> {
    pub fn cognito() -> Self {
        Self {
            authenticator: Arc::new(CognitoAuthenticator::new()),
        }
    }
}

impl AuthState<BuiltInAuthenticator> {
    pub fn built_in(authentication: Option<Authentication>) -> Self {
        Self {
            authenticator: Arc::new(BuiltInAuthenticator::new(authentication)),
        }
    }
}

/// Authentication middleware that validates JWT tokens from Google IAP
pub async fn auth_middleware<T: Authenticator>(
    State(auth_state): State<AuthState<T>>,
    mut request: Request,
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

    // Add user to request extensions for downstream handlers
    request.extensions_mut().insert(user);

    Ok(next.run(request).await)
}
