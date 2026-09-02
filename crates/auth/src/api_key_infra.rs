use crate::api_key_domain::{ApiKeyConfig, ApiKeyService, ValidatedApiKey};
use crate::constants::DEFAULT_API_KEY_HEADER;
use crate::types::Identity;
use axum::http::HeaderMap;
use entity::prelude::Users;
use oxy_platform::db::establish_connection;
use oxy_shared::errors::OxyError;
use sea_orm::{DatabaseConnection, EntityTrait};

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

    let validated_key = ApiKeyService::validate_api_key(db, &key, config).await?;

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

    // Create Identity with real user information.
    //
    // `Identity` is the *provider-shaped* identity — the thing a user is looked
    // up by — so its email is not optional. A user with no mailbox therefore
    // cannot carry an API key, and that is the right answer rather than a gap:
    // API keys are a developer credential, and a frontline worker enrolled by
    // PIN has no path to one. Refusing here beats minting an identity with an
    // empty address that later reaches SES.
    let Some(email) = user.email else {
        tracing::warn!(
            user_id = %validated_key.user_id,
            "API key belongs to a user with no email address; refusing"
        );
        return Err(OxyError::AuthenticationError(
            "API keys require an account with an email address".to_string(),
        ));
    };
    let identity = Identity {
        // An API key already resolved to a row, so name it. Re-resolving by
        // address would be a second lookup that can only agree or be wrong.
        user_id: Some(user.id),
        picture: user.picture,
        email,
        name: Some(user.name),
    };

    Ok((identity, validated_key))
}
