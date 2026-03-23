use crate::server::service::secret_manager::{
    CreateSecretParams, SecretInfo, SecretManagerService, UpdateSecretParams,
};
use axum::{
    extract::{self, Path},
    http::StatusCode,
    response::IntoResponse,
};
use garde::Validate;
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_shared::errors::OxyError;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

#[derive(Serialize)]
pub struct SecretResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by: String,
    pub is_active: bool,
}

impl From<SecretInfo> for SecretResponse {
    fn from(secret: SecretInfo) -> Self {
        Self {
            id: secret.id.to_string(),
            name: secret.name,
            description: secret.description,
            created_at: secret.created_at,
            updated_at: secret.updated_at,
            created_by: secret.created_by.to_string(),
            is_active: secret.is_active,
        }
    }
}

#[derive(Serialize)]
pub struct SecretListResponse {
    pub secrets: Vec<SecretResponse>,
    pub total: usize,
}

#[derive(Deserialize, Validate, Serialize)]
pub struct CreateSecretRequest {
    #[garde(length(min = 1, max = 255))]
    pub name: String,
    #[garde(length(min = 1, max = 10000))]
    pub value: String,
    #[garde(length(min = 0, max = 1000))]
    pub description: Option<String>,
}

#[derive(Deserialize, Validate)]
pub struct BulkCreateSecretsRequest {
    #[garde(length(min = 1, max = 100))]
    pub secrets: Vec<CreateSecretRequest>,
}

#[derive(Serialize)]
pub struct FailedSecret {
    pub secret: CreateSecretRequest,
    pub error: String,
}

#[derive(Serialize)]
pub struct BulkCreateSecretsResponse {
    pub created_secrets: Vec<SecretResponse>,
    pub failed_secrets: Vec<FailedSecret>,
}

#[derive(Deserialize, Validate)]
pub struct UpdateSecretRequest {
    #[garde(length(min = 1, max = 10000))]
    pub value: Option<String>,
    #[garde(length(min = 0, max = 1000))]
    pub description: Option<String>,
}

/// Create a new secret
pub async fn create_secret(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Path(project_id): Path<Uuid>,
    extract::Json(request): extract::Json<CreateSecretRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    // Validate the request using garde
    if let Err(validation_errors) = request.validate() {
        tracing::warn!("Secret creation validation failed: {}", validation_errors);
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            axum::Json(json!({
                "error": "Validation failed",
                "details": validation_errors.to_string()
            })),
        )
            .into_response());
    }

    let db = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let secret_manager = SecretManagerService::new(project_id);

    let create_params = CreateSecretParams {
        name: request.name,
        value: request.value,
        description: request.description,
        created_by: user.id,
    };

    match secret_manager.create_secret(&db, create_params).await {
        Ok(secret_info) => {
            let response = SecretResponse::from(secret_info);
            Ok((StatusCode::CREATED, axum::Json(response)).into_response())
        }
        Err(OxyError::SecretManager(msg)) if msg.contains("already exists") => {
            Ok((StatusCode::CONFLICT, axum::Json(json!({ "error": msg }))).into_response())
        }
        Err(OxyError::SecretManager(msg)) => {
            Ok((StatusCode::BAD_REQUEST, axum::Json(json!({ "error": msg }))).into_response())
        }
        Err(e) => {
            tracing::error!("Failed to create secret: {}", e);
            Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({ "error": "Failed to create secret" })),
            )
                .into_response())
        }
    }
}

/// Create multiple secrets in bulk
pub async fn bulk_create_secrets(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Path(project_id): Path<Uuid>,
    extract::Json(request): extract::Json<BulkCreateSecretsRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    // Validate the request using garde
    if let Err(validation_errors) = request.validate() {
        tracing::warn!(
            "Bulk secret creation validation failed: {}",
            validation_errors
        );
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            axum::Json(json!({
                "error": "Validation failed",
                "details": validation_errors.to_string()
            })),
        )
            .into_response());
    }

    // Validate each individual secret in the request
    for secret_request in &request.secrets {
        if let Err(validation_errors) = secret_request.validate() {
            tracing::warn!("Individual secret validation failed: {}", validation_errors);
            return Ok((
                StatusCode::UNPROCESSABLE_ENTITY,
                axum::Json(json!({
                    "error": "Validation failed for one or more secrets",
                    "details": validation_errors.to_string()
                })),
            )
                .into_response());
        }
    }

    let db = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let secret_manager = SecretManagerService::new(project_id);
    let mut created_secrets = Vec::new();
    let mut failed_secrets = Vec::new();

    // Process each secret individually
    for secret_request in request.secrets {
        let create_params = CreateSecretParams {
            name: secret_request.name.clone(),
            value: secret_request.value.clone(),
            description: secret_request.description.clone(),
            created_by: user.id,
        };

        match secret_manager.create_secret(&db, create_params).await {
            Ok(secret_info) => {
                let response = SecretResponse::from(secret_info);
                created_secrets.push(response);
            }
            Err(e) => {
                let error_msg = match e {
                    OxyError::SecretManager(msg) => msg,
                    _ => "Failed to create secret".to_string(),
                };
                failed_secrets.push(FailedSecret {
                    secret: secret_request,
                    error: error_msg,
                });
            }
        }
    }

    let response = BulkCreateSecretsResponse {
        created_secrets,
        failed_secrets,
    };

    // Return 201 if all succeeded, 207 (Multi-Status) if partial success, 400 if all failed
    let status_code = if response.failed_secrets.is_empty() {
        StatusCode::CREATED
    } else if !response.created_secrets.is_empty() {
        StatusCode::MULTI_STATUS
    } else {
        StatusCode::BAD_REQUEST
    };

    Ok((status_code, axum::Json(response)).into_response())
}

/// List all secrets (without values)
pub async fn list_secrets(
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let secret_manager = SecretManagerService::new(project_id);
    match secret_manager.list_secrets(&db).await {
        Ok(secrets) => {
            let secret_responses: Vec<SecretResponse> =
                secrets.into_iter().map(SecretResponse::from).collect();

            let response = SecretListResponse {
                total: secret_responses.len(),
                secrets: secret_responses,
            };

            Ok((StatusCode::OK, axum::Json(response)).into_response())
        }
        Err(e) => {
            tracing::error!("Failed to list secrets: {}", e);
            Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({ "error": "Failed to list secrets" })),
            )
                .into_response())
        }
    }
}

/// Get secret metadata by ID (without value)
pub async fn get_secret(
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path((project_id, id)): Path<(Uuid, String)>,
) -> Result<impl IntoResponse, StatusCode> {
    let secret_id = match Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                axum::Json(json!({ "error": "Invalid secret ID format" })),
            )
                .into_response());
        }
    };

    let db = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let secret_manager = SecretManagerService::new(project_id);

    // Find secret by ID from the list of secrets
    match secret_manager.list_secrets(&db).await {
        Ok(secrets) => {
            if let Some(secret) = secrets.into_iter().find(|s| s.id == secret_id) {
                let response = SecretResponse::from(secret);
                Ok((StatusCode::OK, axum::Json(response)).into_response())
            } else {
                Ok((
                    StatusCode::NOT_FOUND,
                    axum::Json(json!({ "error": "Secret not found" })),
                )
                    .into_response())
            }
        }
        Err(e) => {
            tracing::error!("Failed to get secret: {}", e);
            Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({ "error": "Failed to get secret" })),
            )
                .into_response())
        }
    }
}

/// Update a secret by ID
pub async fn update_secret(
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path((project_id, id)): Path<(Uuid, String)>,
    extract::Json(request): extract::Json<UpdateSecretRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    // Validate the request using garde
    if let Err(validation_errors) = request.validate() {
        tracing::warn!("Secret update validation failed: {}", validation_errors);
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            axum::Json(json!({
                "error": "Validation failed",
                "details": validation_errors.to_string()
            })),
        )
            .into_response());
    }

    let secret_id = match Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                axum::Json(json!({ "error": "Invalid secret ID format" })),
            )
                .into_response());
        }
    };

    let db = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let secret_manager = SecretManagerService::new(project_id);

    // Find secret by ID to get the name
    match secret_manager.list_secrets(&db).await {
        Ok(secrets) => {
            if let Some(secret) = secrets.into_iter().find(|s| s.id == secret_id) {
                let update_params = UpdateSecretParams {
                    value: request.value,
                    description: request.description,
                };

                match secret_manager
                    .update_secret(&db, &secret.name, update_params)
                    .await
                {
                    Ok(updated_secret) => {
                        let response = SecretResponse::from(updated_secret);
                        Ok((StatusCode::OK, axum::Json(response)).into_response())
                    }
                    Err(OxyError::SecretManager(msg)) => Ok((
                        StatusCode::BAD_REQUEST,
                        axum::Json(json!({ "error": msg })),
                    )
                        .into_response()),
                    Err(e) => {
                        tracing::error!("Failed to update secret: {}", e);
                        Ok((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            axum::Json(json!({ "error": "Failed to update secret" })),
                        )
                            .into_response())
                    }
                }
            } else {
                Ok((
                    StatusCode::NOT_FOUND,
                    axum::Json(json!({ "error": "Secret not found" })),
                )
                    .into_response())
            }
        }
        Err(e) => {
            tracing::error!("Failed to find secret: {}", e);
            Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({ "error": "Failed to find secret" })),
            )
                .into_response())
        }
    }
}

/// Delete a secret by ID
pub async fn delete_secret(
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path((project_id, id)): Path<(Uuid, String)>,
) -> Result<impl IntoResponse, StatusCode> {
    let secret_id = match Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                axum::Json(json!({ "error": "Invalid secret ID format" })),
            )
                .into_response());
        }
    };

    let db = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let secret_manager = SecretManagerService::new(project_id);

    // Find secret by ID to get the name
    match secret_manager.list_secrets(&db).await {
        Ok(secrets) => {
            if let Some(secret) = secrets.into_iter().find(|s| s.id == secret_id) {
                match secret_manager.delete_secret(&db, &secret.name).await {
                    Ok(()) => Ok((StatusCode::NO_CONTENT, axum::Json(json!({}))).into_response()),
                    Err(OxyError::SecretManager(msg)) => Ok((
                        StatusCode::BAD_REQUEST,
                        axum::Json(json!({ "error": msg })),
                    )
                        .into_response()),
                    Err(e) => {
                        tracing::error!("Failed to delete secret: {}", e);
                        Ok((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            axum::Json(json!({ "error": "Failed to delete secret" })),
                        )
                            .into_response())
                    }
                }
            } else {
                Ok((
                    StatusCode::NOT_FOUND,
                    axum::Json(json!({ "error": "Secret not found" })),
                )
                    .into_response())
            }
        }
        Err(e) => {
            tracing::error!("Failed to find secret: {}", e);
            Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({ "error": "Failed to find secret" })),
            )
                .into_response())
        }
    }
}
