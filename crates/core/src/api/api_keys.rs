use crate::{
    auth::extractor::AuthenticatedUserExtractor,
    db::client::establish_connection,
    errors::OxyError,
    service::api_key::{ApiKeyConfig, ApiKeyService, CreateApiKeyParams, CreateApiKeyResponse},
};
use axum::{
    extract::{self, Path},
    http::StatusCode,
    response::IntoResponse,
};
use entity::api_keys::Model as ApiKeyModel;
use garde::Validate;
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;
use uuid::Uuid;

// Validation functions for expires_at fields
fn validate_expires_at(
    value: &Option<chrono::DateTime<chrono::Utc>>,
    _context: &(),
) -> garde::Result {
    if let Some(expires_at) = value {
        let now = chrono::Utc::now();
        if *expires_at <= now {
            return Err(garde::Error::new("expires_at must be in the future"));
        }
    }
    Ok(())
}

// Helper function to mask API keys for safe display
fn mask_api_key(key: &str) -> String {
    if key.len() <= 8 {
        // If key is too short, just show asterisks
        "*".repeat(key.len())
    } else {
        // Show first 4 and last 4 characters, mask the middle
        let start = &key[..4];
        let end = &key[key.len() - 4..];
        let middle_len = key.len() - 8;
        format!("{}{}...{}", start, "*".repeat(middle_len.min(8)), end)
    }
}

#[derive(Serialize, ToSchema)]
pub struct ApiKeyResponse {
    pub id: String,
    pub name: String,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub is_active: bool,
    #[schema(example = "sk_1234****...5678")]
    pub masked_key: Option<String>, // Only shown for newly created keys or when explicitly requested
}

impl ApiKeyResponse {
    // Create a new response with a masked key (for newly created keys)
    pub fn with_masked_key(mut self, key: &str) -> Self {
        self.masked_key = Some(mask_api_key(key));
        self
    }

    // Create a response without any key information (for listing existing keys)
    pub fn without_key(mut self) -> Self {
        self.masked_key = None;
        self
    }
}

#[derive(Serialize, ToSchema)]
pub struct ApiKeyListResponse {
    pub api_keys: Vec<ApiKeyResponse>,
    pub total: usize,
}

#[derive(Deserialize, ToSchema, Validate)]
pub struct CreateApiKeyRequest {
    #[garde(length(min = 1, max = 100))]
    #[schema(example = "My Production API Key")]
    pub name: String,

    #[garde(custom(validate_expires_at))]
    #[schema(example = "2025-12-31T23:59:59Z")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize, ToSchema)]
pub struct CreateApiKeyResponseDto {
    pub id: String,
    #[schema(example = "sk_1234567890abcdef...")]
    pub key: String, // Full key only shown on creation
    #[schema(example = "sk_1234****...cdef")]
    pub masked_key: String, // Masked version for safer display
    pub name: String,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<CreateApiKeyResponse> for CreateApiKeyResponseDto {
    fn from(response: CreateApiKeyResponse) -> Self {
        let masked = mask_api_key(&response.key);

        Self {
            id: response.id.to_string(),
            key: response.key.clone(),
            masked_key: masked,
            name: response.name,
            expires_at: response.expires_at,
            created_at: response.created_at,
        }
    }
}

impl From<ApiKeyModel> for ApiKeyResponse {
    fn from(model: ApiKeyModel) -> Self {
        Self {
            id: model.id.to_string(),
            name: model.name,
            expires_at: model.expires_at.map(|dt| dt.into()),
            last_used_at: model.last_used_at.map(|dt| dt.into()),
            created_at: model.created_at.into(),
            is_active: model.is_active,
            masked_key: None, // Never expose key data from stored models
        }
    }
}

/// Create a new API key
#[utoipa::path(
    post,
    path = "/api-keys",
    request_body = CreateApiKeyRequest,
    responses(
        (status = 201, description = "API key created successfully", body = CreateApiKeyResponseDto),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 422, description = "Validation error"),
        (status = 500, description = "Internal server error")
    ),
    tag = "API Keys"
)]
pub async fn create_api_key(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Path(project_id): Path<Uuid>,
    extract::Json(request): extract::Json<CreateApiKeyRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    // Validate the request using garde
    if let Err(validation_errors) = request.validate() {
        tracing::warn!("API key creation validation failed: {}", validation_errors);
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            extract::Json(json!({
                "error": "Validation failed",
                "details": validation_errors.to_string()
            })),
        )
            .into_response());
    }

    let db = establish_connection().await?;
    let config = ApiKeyConfig::default();

    let create_request = CreateApiKeyParams {
        user_id: user.id,
        name: request.name,
        expires_at: request.expires_at,
        project_id,
    };

    match ApiKeyService::create_api_key(&db, create_request, &config).await {
        Ok(response) => {
            let dto: CreateApiKeyResponseDto = response.into();
            Ok((StatusCode::CREATED, extract::Json(dto)).into_response())
        }
        Err(OxyError::ValidationError(msg)) => {
            tracing::error!("API key service validation error: {}", msg);
            Ok((
                StatusCode::BAD_REQUEST,
                extract::Json(json!({
                    "error": msg
                })),
            )
                .into_response())
        }
        Err(e) => {
            tracing::error!("Failed to create API key: {}", e);
            Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                extract::Json(json!({
                    "error": "Internal server error"
                })),
            )
                .into_response())
        }
    }
}

/// List user's API keys
#[utoipa::path(
    get,
    path = "/api-keys",
    responses(
        (status = 200, description = "List of API keys", body = ApiKeyListResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "API Keys"
)]
pub async fn list_api_keys(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<impl IntoResponse, StatusCode> {
    let db = establish_connection().await?;

    match ApiKeyService::list_user_api_keys(&db, user.id).await {
        Ok(api_keys) => {
            let api_key_responses: Vec<ApiKeyResponse> = api_keys
                .into_iter()
                .map(|key| ApiKeyResponse::from(key).without_key())
                .collect();

            let response = ApiKeyListResponse {
                total: api_key_responses.len(),
                api_keys: api_key_responses,
            };

            Ok(extract::Json(response))
        }
        Err(e) => {
            tracing::error!("Failed to list API keys: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get specific API key info
#[utoipa::path(
    get,
    path = "/api-keys/{id}",
    params(
        ("id" = String, Path, description = "API key ID")
    ),
    responses(
        (status = 200, description = "API key details", body = ApiKeyResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "API key not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "API Keys"
)]
pub async fn get_api_key(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let key_id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let db = establish_connection().await?;

    // Get all user's API keys and find the requested one
    match ApiKeyService::list_user_api_keys(&db, user.id).await {
        Ok(api_keys) => {
            if let Some(api_key) = api_keys.into_iter().find(|k| k.id == key_id) {
                let response = ApiKeyResponse::from(api_key).without_key();
                Ok(extract::Json(response))
            } else {
                Err(StatusCode::NOT_FOUND)
            }
        }
        Err(e) => {
            tracing::error!("Failed to get API key: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Revoke (delete) an API key
#[utoipa::path(
    delete,
    path = "/api-keys/{id}",
    params(
        ("id" = String, Path, description = "API key ID")
    ),
    responses(
        (status = 204, description = "API key revoked successfully"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "API key not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "API Keys"
)]
pub async fn delete_api_key(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let key_id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let db = establish_connection().await?;

    match ApiKeyService::revoke_api_key(&db, key_id, user.id).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(OxyError::ValidationError(_)) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to revoke API key: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
