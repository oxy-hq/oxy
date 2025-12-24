use crate::auth::types::Identity;
use crate::config::constants::DEFAULT_API_KEY_HEADER;
use crate::db::client::establish_connection;
use crate::errors::OxyError;
use crate::service::api_key::{ApiKeyConfig, ApiKeyService, ValidatedApiKey};
use axum::http::HeaderMap;
use entity::prelude::Users;
use sea_orm::{DatabaseConnection, EntityTrait};

fn extract_api_key_from_headers(headers: &HeaderMap) -> Option<String> {
    tracing::debug!(
        "Checking headers for API key header '{}'",
        DEFAULT_API_KEY_HEADER
    );
    extract_api_key_from_headers_with_name(headers, DEFAULT_API_KEY_HEADER)
}

fn extract_api_key_from_headers_with_name(
    headers: &HeaderMap,
    header_name: &str,
) -> Option<String> {
    tracing::debug!("Checking headers for API key header '{}'", header_name);
    headers
        .get(header_name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
}

pub async fn authenticate_header(headers: &HeaderMap) -> Result<Identity, OxyError> {
    // Establish database connection
    let db = establish_connection().await.map_err(|e| {
        tracing::error!(
            "Failed to establish database connection for API key validation: {}",
            e
        );
        OxyError::AuthenticationError("Failed to validate API key".to_string())
    })?;

    let config = ApiKeyConfig::default();

    let (identity, _) =
        authenticate_header_with_config(&db, headers, DEFAULT_API_KEY_HEADER, &config).await?;

    Ok(identity)
}

pub async fn authenticate_header_with_config(
    db: &DatabaseConnection,
    headers: &HeaderMap,
    header_name: &str,
    config: &ApiKeyConfig,
) -> Result<(Identity, ValidatedApiKey), OxyError> {
    let key = extract_api_key_from_headers_with_name(headers, header_name).ok_or_else(|| {
        OxyError::AuthenticationError(format!(
            "No API key found in headers (expected: {})",
            header_name
        ))
    })?;

    // Validate the API key
    let validated_key = ApiKeyService::validate_api_key(db, &key, config)
        .await?
        .ok_or_else(|| {
            tracing::warn!("Invalid or expired API key provided");
            OxyError::AuthenticationError("Invalid or expired API key".to_string())
        })?;

    // Get the user associated with the API key
    let user = Users::find_by_id(validated_key.user_id)
        .one(db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch user for API key: {}", e);
            OxyError::AuthenticationError("Failed to authenticate user".to_string())
        })?
        .ok_or_else(|| {
            tracing::error!(
                "User not found for validated API key: {}",
                validated_key.user_id
            );
            OxyError::AuthenticationError("User not found".to_string())
        })?;

    // Create Identity with real user information
    let identity = Identity {
        idp_id: Some(validated_key.id.to_string()), // Use the API key ID as the identity provider ID
        picture: user.picture,
        email: user.email,
        name: Some(user.name),
    };

    Ok((identity, validated_key))
}
