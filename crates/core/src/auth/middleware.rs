use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::env;
use std::sync::Arc;

use crate::{
    auth::{cognito::CognitoValidator, iap::IAPValidator, user::UserService},
    config::{auth::Authentication, constants::GCP_IAP_AUD_ENV_VAR},
    errors::OxyError,
};

use super::{build_in::BuildInValidator, validator::Validator};

pub struct AuthState<T> {
    validator: Arc<T>,
}

impl<T> Clone for AuthState<T> {
    fn clone(&self) -> Self {
        Self {
            validator: Arc::clone(&self.validator),
        }
    }
}

impl AuthState<IAPValidator> {
    pub fn iap() -> Result<Self, OxyError> {
        let audience = env::var(GCP_IAP_AUD_ENV_VAR).map_err(|err| {
            OxyError::ConfigurationError(format!(
                "Failed to read {} environment variable: {}",
                GCP_IAP_AUD_ENV_VAR, err
            ))
        })?;

        Ok(Self {
            validator: Arc::new(IAPValidator::new(audience, false)),
        })
    }

    pub fn iap_cloud_run() -> Self {
        Self {
            validator: Arc::new(IAPValidator::new("".to_string(), true)),
        }
    }
}

impl AuthState<CognitoValidator> {
    pub fn cognito() -> Self {
        Self {
            validator: Arc::new(CognitoValidator::new()),
        }
    }
}

impl AuthState<BuildInValidator> {
    pub fn built_in(authentication: Option<Authentication>) -> Self {
        Self {
            validator: Arc::new(BuildInValidator::new(authentication)),
        }
    }
}

/// Authentication middleware that validates JWT tokens from Google IAP
pub async fn auth_middleware<T: Validator>(
    State(auth_state): State<AuthState<T>>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let headers = request.headers();

    // Validate the JWT token from headers
    let claims = auth_state.validator.verify(headers).map_err(|err| {
        tracing::error!("JWT validation failed: {}", err);
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
