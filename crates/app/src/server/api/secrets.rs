use crate::server::api::middlewares::project::ProjectManagerExtractor;
use crate::server::service::secret_manager::{
    CreateSecretParams, SecretInfo, SecretManagerService, UpdateSecretParams,
};
use axum::{
    extract::{self, Path},
    http::StatusCode,
    response::IntoResponse,
};
use entity::users::Entity as Users;
use garde::Validate;
use oxy::config::model::IntegrationType;
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_shared::errors::OxyError;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Serialize)]
pub struct SecretResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by: String,
    pub created_by_email: Option<String>,
    pub updated_by_email: Option<String>,
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
            created_by_email: None, // populated by list_secrets after user lookup
            updated_by_email: None, // populated by list_secrets after user lookup
            is_active: secret.is_active,
        }
    }
}

/// Resolve a set of user UUIDs to their emails in one query.
async fn resolve_user_emails(
    db: &impl sea_orm::ConnectionTrait,
    ids: &[Uuid],
) -> HashMap<Uuid, String> {
    if ids.is_empty() {
        return HashMap::new();
    }
    match Users::find()
        .filter(entity::users::Column::Id.is_in(ids.to_vec()))
        .all(db)
        .await
    {
        Ok(users) => users.into_iter().map(|u| (u.id, u.email)).collect(),
        Err(e) => {
            tracing::warn!("Failed to resolve user emails: {}", e);
            HashMap::new()
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
    #[garde(length(min = 1, max = 100), dive)]
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
            // Resolve created_by and updated_by UUIDs → emails in one batch query
            let user_ids: Vec<Uuid> = secrets
                .iter()
                .flat_map(|s| std::iter::once(s.created_by).chain(s.updated_by))
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            let email_map = resolve_user_emails(&db, &user_ids).await;

            let secret_responses: Vec<SecretResponse> = secrets
                .into_iter()
                .map(|s| {
                    let created_by_email = email_map.get(&s.created_by).cloned();
                    let updated_by_email = s.updated_by.and_then(|id| email_map.get(&id).cloned());
                    SecretResponse {
                        created_by_email,
                        updated_by_email,
                        ..SecretResponse::from(s)
                    }
                })
                .collect();

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

    let secret_manager = SecretManagerService::new(project_id);

    match secret_manager.get_secret_by_id(secret_id).await {
        Some(secret) => {
            Ok((StatusCode::OK, axum::Json(SecretResponse::from(secret))).into_response())
        }
        None => Ok((
            StatusCode::NOT_FOUND,
            axum::Json(json!({ "error": "Secret not found" })),
        )
            .into_response()),
    }
}

/// Update a secret by ID
pub async fn update_secret(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Path((project_id, id)): Path<(Uuid, String)>,
    extract::Json(request): extract::Json<UpdateSecretRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    if request.value.is_none() && request.description.is_none() {
        return Ok((
            StatusCode::BAD_REQUEST,
            axum::Json(
                json!({ "error": "At least one of 'value' or 'description' must be provided" }),
            ),
        )
            .into_response());
    }

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

    let Some(secret) = secret_manager.get_secret_by_id(secret_id).await else {
        return Ok((
            StatusCode::NOT_FOUND,
            axum::Json(json!({ "error": "Secret not found" })),
        )
            .into_response());
    };

    let update_params = UpdateSecretParams {
        value: request.value,
        description: request.description,
        updated_by: user.id,
    };

    match secret_manager
        .update_secret(&db, &secret.name, update_params)
        .await
    {
        Ok(updated_secret) => Ok((
            StatusCode::OK,
            axum::Json(SecretResponse::from(updated_secret)),
        )
            .into_response()),
        Err(OxyError::SecretManager(msg)) => {
            Ok((StatusCode::BAD_REQUEST, axum::Json(json!({ "error": msg }))).into_response())
        }
        Err(e) => {
            tracing::error!("Failed to update secret: {}", e);
            Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({ "error": "Failed to update secret" })),
            )
                .into_response())
        }
    }
}

/// Masks a secret value for safe display.
/// Shows first 4 + variable stars + last 4 for values longer than 8 chars; otherwise all stars.
/// Uses char-boundary safe indexing to handle multi-byte UTF-8 characters.
fn mask_secret_value(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len();
    if len <= 8 {
        return "*".repeat(len);
    }
    let start: String = chars[..4].iter().collect();
    let end: String = chars[len - 4..].iter().collect();
    format!("{}{}{}", start, "*".repeat(len - 8), end)
}

/// Parses a `.env` file and returns `(key, value)` pairs.
/// Skips blank lines, comment lines (`#`), and lines without `=`.
fn parse_dotenv_file(path: &std::path::Path) -> Vec<(String, String)> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let eq_pos = line.find('=')?;
            let key = line[..eq_pos].trim().to_string();
            if key.is_empty() {
                return None;
            }
            let raw = line[eq_pos + 1..].to_string();
            // Strip surrounding single or double quotes; for unquoted values strip inline comments
            let value = if raw.len() >= 2
                && ((raw.starts_with('"') && raw.ends_with('"'))
                    || (raw.starts_with('\'') && raw.ends_with('\'')))
            {
                raw[1..raw.len() - 1].to_string()
            } else {
                // Strip inline comment: anything after the first " #" (space + hash)
                let stripped = raw.split(" #").next().unwrap_or(&raw).trim_end();
                stripped.to_string()
            };
            Some((key, value))
        })
        .collect()
}

/// Info about a secret that is sourced from an environment variable reference in config.yml.
#[derive(Serialize)]
pub struct EnvSecretInfo {
    /// The environment variable name (e.g. "SLACK_BOT_TOKEN")
    pub env_var: String,
    /// The config field that references this env var (e.g. "slack.bot_token_var")
    pub config_field: String,
    /// Whether the environment variable is currently set (non-empty value)
    pub is_set: bool,
    /// Masked value of the env var if set (e.g. "sk-a****bcde"), None if not set
    pub masked_value: Option<String>,
    /// Full plaintext value — present because this endpoint requires admin access.
    pub full_value: Option<String>,
}

/// List all environment-variable-referenced secrets from config.yml.
///
/// Scans the project config for `*_var` fields and returns their status.
/// This helps users see which secrets are being read from the environment
/// and potentially override them with database-stored secrets.
pub async fn list_env_secrets(
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path(_project_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let config = project_manager.config_manager.get_config();
    let mut env_secrets: Vec<EnvSecretInfo> = Vec::new();

    let mut seen_vars: std::collections::HashSet<String> = std::collections::HashSet::new();

    let make_config_entry = |env_var: String, config_field: String| {
        let value = std::env::var(&env_var).ok().filter(|v| !v.is_empty());
        let is_set = value.is_some();
        let masked_value = value.as_deref().map(mask_secret_value);
        let full_value = value.clone();
        EnvSecretInfo {
            env_var,
            config_field,
            is_set,
            masked_value,
            full_value,
        }
    };

    // Scan Slack settings
    if let Some(slack) = &config.slack {
        if let Some(var) = &slack.bot_token_var {
            seen_vars.insert(var.clone());
            env_secrets.push(make_config_entry(
                var.clone(),
                "slack.bot_token_var".to_string(),
            ));
        }
        if let Some(var) = &slack.signing_secret_var {
            seen_vars.insert(var.clone());
            env_secrets.push(make_config_entry(
                var.clone(),
                "slack.signing_secret_var".to_string(),
            ));
        }
    }

    // Scan integrations
    for integration in &config.integrations {
        match &integration.integration_type {
            IntegrationType::Omni(omni) => {
                seen_vars.insert(omni.api_key_var.clone());
                env_secrets.push(make_config_entry(
                    omni.api_key_var.clone(),
                    format!("integrations.{}.api_key_var", integration.name),
                ));
            }
            IntegrationType::Looker(looker) => {
                seen_vars.insert(looker.client_id_var.clone());
                env_secrets.push(make_config_entry(
                    looker.client_id_var.clone(),
                    format!("integrations.{}.client_id_var", integration.name),
                ));
                seen_vars.insert(looker.client_secret_var.clone());
                env_secrets.push(make_config_entry(
                    looker.client_secret_var.clone(),
                    format!("integrations.{}.client_secret_var", integration.name),
                ));
            }
        }
    }

    // Parse .env file and add vars not already covered by config references.
    // is_set and masked_value are derived from the live process environment
    // (std::env::var), not the file contents, so the status reflects what the
    // running server actually sees regardless of how the .env file was loaded.
    let dotenv_path = project_manager.config_manager.project_path().join(".env");
    for (key, _file_value) in parse_dotenv_file(&dotenv_path) {
        if seen_vars.contains(&key) {
            continue;
        }
        let live_value = std::env::var(&key).ok().filter(|v| !v.is_empty());
        let is_set = live_value.is_some();
        let masked_value = live_value.as_deref().map(mask_secret_value);
        let full_value = live_value.clone();
        env_secrets.push(EnvSecretInfo {
            env_var: key,
            config_field: ".env".to_string(),
            is_set,
            masked_value,
            full_value,
        });
    }

    Ok((StatusCode::OK, axum::Json(env_secrets)).into_response())
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

    let Some(secret) = secret_manager.get_secret_by_id(secret_id).await else {
        return Ok((
            StatusCode::NOT_FOUND,
            axum::Json(json!({ "error": "Secret not found" })),
        )
            .into_response());
    };

    match secret_manager.delete_secret(&db, &secret.name).await {
        Ok(()) => Ok((StatusCode::NO_CONTENT, axum::Json(json!({}))).into_response()),
        Err(OxyError::SecretManager(msg)) => {
            Ok((StatusCode::BAD_REQUEST, axum::Json(json!({ "error": msg }))).into_response())
        }
        Err(e) => {
            tracing::error!("Failed to delete secret: {}", e);
            Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({ "error": "Failed to delete secret" })),
            )
                .into_response())
        }
    }
}

/// Reveal the plaintext value of a DB secret by ID (admin-only via middleware).
pub async fn reveal_secret(
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

    let secret_manager = SecretManagerService::new(project_id);

    match secret_manager.get_secret_value_by_id(secret_id).await {
        Some(value) => Ok((StatusCode::OK, axum::Json(json!({ "value": value }))).into_response()),
        None => Ok((
            StatusCode::NOT_FOUND,
            axum::Json(json!({ "error": "Secret not found or decryption failed" })),
        )
            .into_response()),
    }
}
