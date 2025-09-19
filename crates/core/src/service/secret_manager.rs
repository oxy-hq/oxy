use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use base64::{Engine as _, engine::general_purpose};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{db::client::establish_connection, errors::OxyError, utils::get_encryption_key};
use entity::secrets::{self, ActiveModel as SecretActiveModel, Entity as Secret};

#[derive(Debug, Clone)]
pub struct SecretManagerService {
    encryption_key: [u8; 32],
    cache: Arc<RwLock<HashMap<String, CachedSecret>>>,
    project_id: Uuid,
}

#[derive(Debug, Clone)]
struct CachedSecret {
    value: String,
    cached_at: chrono::DateTime<chrono::Utc>,
    ttl_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct CreateSecretParams {
    pub name: String,
    pub value: String,
    pub description: Option<String>,
    pub created_by: Uuid,
}

#[derive(Debug, Clone)]
pub struct UpdateSecretParams {
    pub value: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SecretInfo {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by: Uuid,
    pub is_active: bool,
}

impl SecretManagerService {
    pub fn new(project_id: Uuid) -> Self {
        let encryption_key = get_encryption_key();
        Self {
            encryption_key,
            cache: Arc::new(RwLock::new(HashMap::new())),
            project_id: project_id,
        }
    }

    /// Encrypt a secret value
    fn encrypt_value(&self, value: &str) -> Result<String, OxyError> {
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.encryption_key));
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

        let ciphertext = cipher
            .encrypt(&nonce, value.as_bytes())
            .map_err(|e| OxyError::SecretManager(format!("Encryption failed: {e}")))?;

        // Combine nonce and ciphertext, then base64 encode
        let mut combined = nonce.to_vec();
        combined.extend_from_slice(&ciphertext);

        Ok(general_purpose::STANDARD.encode(&combined))
    }

    /// Decrypt a secret value
    fn decrypt_value(&self, encrypted_value: &str) -> Result<String, OxyError> {
        let combined = general_purpose::STANDARD
            .decode(encrypted_value)
            .map_err(|e| OxyError::SecretManager(format!("Invalid encrypted value format: {e}")))?;

        if combined.len() < 12 {
            return Err(OxyError::SecretManager(
                "Invalid encrypted value: too short".to_string(),
            ));
        }

        let (nonce_bytes, ciphertext) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.encryption_key));
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| OxyError::SecretManager(format!("Decryption failed: {e}")))?;

        String::from_utf8(plaintext)
            .map_err(|e| OxyError::SecretManager(format!("Invalid UTF-8 in decrypted value: {e}")))
    }

    /// Validate secret name
    fn validate_secret_name(name: &str) -> Result<(), OxyError> {
        if name.is_empty() {
            return Err(OxyError::SecretManager(
                "Secret name cannot be empty".to_string(),
            ));
        }

        if name.len() > 255 {
            return Err(OxyError::SecretManager(
                "Secret name cannot be longer than 255 characters".to_string(),
            ));
        }

        // Check for valid characters (alphanumeric, underscore, hyphen, dot)
        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
        {
            return Err(OxyError::SecretManager(
                "Secret name can only contain alphanumeric characters, underscores, hyphens, and dots".to_string(),
            ));
        }

        Ok(())
    }

    /// Sanitize secret value
    fn sanitize_secret_value(value: &str) -> Result<String, OxyError> {
        if value.is_empty() {
            return Err(OxyError::SecretManager(
                "Secret value cannot be empty".to_string(),
            ));
        }

        if value.len() > 10000 {
            return Err(OxyError::SecretManager(
                "Secret value cannot be longer than 10000 characters".to_string(),
            ));
        }

        // Trim whitespace
        Ok(value.trim().to_string())
    }

    /// Create a new secret
    pub async fn create_secret(
        &self,
        db: &DatabaseConnection,
        params: CreateSecretParams,
    ) -> Result<SecretInfo, OxyError> {
        tracing::info!("Creating secret: {}", params.name);
        Self::validate_secret_name(&params.name)?;
        let sanitized_value = Self::sanitize_secret_value(&params.value)?;

        // Check if secret with this name already exists
        let existing = Secret::find()
            .filter(secrets::Column::Name.eq(&params.name))
            .filter(secrets::Column::ProjectId.eq(self.project_id))
            .filter(secrets::Column::IsActive.eq(true))
            .one(db)
            .await
            .map_err(|e| OxyError::Database(e.to_string()))?;

        tracing::debug!(
            "Checking for existing secret with name '{}': {:?}",
            params.name,
            existing
        );
        if existing.is_some() {
            tracing::warn!(
                "Attempted to create secret with duplicate name: {}",
                params.name
            );
            return Err(OxyError::SecretManager(format!(
                "Secret with name '{}' already exists",
                params.name
            )));
        }

        let encrypted_value = self.encrypt_value(&sanitized_value)?;
        let now = chrono::Utc::now();

        let secret_model = SecretActiveModel {
            id: Set(Uuid::new_v4()),
            name: Set(params.name.clone()),
            encrypted_value: Set(encrypted_value),
            description: Set(params.description),
            created_at: Set(now.into()),
            updated_at: Set(now.into()),
            created_by: Set(params.created_by),
            is_active: Set(true),
            project_id: Set(self.project_id),
        };

        tracing::info!("Inserting new secret: {:?}", secret_model);

        let saved_secret = secret_model.insert(db).await.map_err(|e| {
            tracing::error!("Failed to insert secret: {}", e);
            OxyError::Database(e.to_string())
        })?;

        tracing::info!("Secret created successfully: {:?}", saved_secret);
        // Invalidate cache for this secret
        self.invalidate_cache(&params.name).await;

        Ok(SecretInfo {
            id: saved_secret.id,
            name: saved_secret.name,
            description: saved_secret.description,
            created_at: saved_secret.created_at.into(),
            updated_at: saved_secret.updated_at.into(),
            created_by: saved_secret.created_by,
            is_active: saved_secret.is_active,
        })
    }

    /// Get a secret value by name
    pub async fn get_secret(&self, name: &str) -> Option<String> {
        // Check cache first
        if let Some(cached_value) = self.get_from_cache(name).await {
            return Some(cached_value);
        }

        let db = establish_connection().await;

        let secret = match db {
            Ok(conn) => {
                let rs = Secret::find()
                    .filter(secrets::Column::Name.eq(name))
                    .filter(secrets::Column::IsActive.eq(true))
                    .filter(secrets::Column::ProjectId.eq(self.project_id))
                    .one(&conn)
                    .await;
                match rs {
                    Ok(secret) => secret,
                    Err(e) => {
                        tracing::error!("Failed to query secret: {}", e);
                        return None;
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to establish database connection: {}", e);
                return None;
            }
        };

        if let Some(secret) = secret {
            let decrypted_value = self.decrypt_value(&secret.encrypted_value);
            match decrypted_value {
                Ok(value) => {
                    // Cache the decrypted value
                    self.cache_value(name, &value).await;

                    Some(value)
                }
                Err(e) => {
                    tracing::error!("Failed to decrypt secret value: {}", e);
                    None
                }
            }
        } else {
            None
        }
    }

    /// List all secrets (without values)
    pub async fn list_secrets(&self, db: &DatabaseConnection) -> Result<Vec<SecretInfo>, OxyError> {
        let secrets = Secret::find()
            .filter(secrets::Column::IsActive.eq(true))
            .filter(secrets::Column::ProjectId.eq(self.project_id))
            .order_by_asc(secrets::Column::Name)
            .all(db)
            .await
            .map_err(|e| OxyError::Database(e.to_string()))?;

        Ok(secrets
            .into_iter()
            .map(|secret| SecretInfo {
                id: secret.id,
                name: secret.name,
                description: secret.description,
                created_at: secret.created_at.into(),
                updated_at: secret.updated_at.into(),
                created_by: secret.created_by,
                is_active: secret.is_active,
            })
            .collect())
    }

    /// Update a secret
    pub async fn update_secret(
        &self,
        db: &DatabaseConnection,
        name: &str,
        params: UpdateSecretParams,
    ) -> Result<SecretInfo, OxyError> {
        let secret = Secret::find()
            .filter(secrets::Column::Name.eq(name))
            .filter(secrets::Column::IsActive.eq(true))
            .one(db)
            .await
            .map_err(|e| OxyError::Database(e.to_string()))?;

        let secret = secret.ok_or_else(|| {
            OxyError::SecretManager(format!("Secret with name '{name}' not found"))
        })?;

        let mut secret_model: SecretActiveModel = secret.into();

        if let Some(new_value) = params.value {
            let sanitized_value = Self::sanitize_secret_value(&new_value)?;
            let encrypted_value = self.encrypt_value(&sanitized_value)?;
            secret_model.encrypted_value = Set(encrypted_value);
        }

        if let Some(new_description) = params.description {
            secret_model.description = Set(Some(new_description));
        }

        secret_model.updated_at = Set(chrono::Utc::now().into());

        let updated_secret = secret_model
            .update(db)
            .await
            .map_err(|e| OxyError::Database(e.to_string()))?;

        // Invalidate cache for this secret
        self.invalidate_cache(name).await;

        Ok(SecretInfo {
            id: updated_secret.id,
            name: updated_secret.name,
            description: updated_secret.description,
            created_at: updated_secret.created_at.into(),
            updated_at: updated_secret.updated_at.into(),
            created_by: updated_secret.created_by,
            is_active: updated_secret.is_active,
        })
    }

    /// Delete a secret (soft delete)
    pub async fn delete_secret(&self, db: &DatabaseConnection, name: &str) -> Result<(), OxyError> {
        let secret = Secret::find()
            .filter(secrets::Column::Name.eq(name))
            .filter(secrets::Column::IsActive.eq(true))
            .one(db)
            .await
            .map_err(|e| OxyError::Database(e.to_string()))?;

        let secret = secret.ok_or_else(|| {
            OxyError::SecretManager(format!("Secret with name '{name}' not found"))
        })?;

        let mut secret_model: SecretActiveModel = secret.into();
        secret_model.is_active = Set(false);
        secret_model.updated_at = Set(chrono::Utc::now().into());

        secret_model
            .update(db)
            .await
            .map_err(|e| OxyError::Database(e.to_string()))?;

        // Remove from cache
        self.invalidate_cache(name).await;

        Ok(())
    }

    // Cache management methods
    async fn get_from_cache(&self, name: &str) -> Option<String> {
        let cache = self.cache.read().await;
        if let Some(cached) = cache.get(name) {
            let now = chrono::Utc::now();
            let age = now.timestamp() as u64 - cached.cached_at.timestamp() as u64;

            if age < cached.ttl_seconds {
                return Some(cached.value.clone());
            }
        }
        None
    }

    async fn cache_value(&self, name: &str, value: &str) {
        let mut cache = self.cache.write().await;
        cache.insert(
            name.to_string(),
            CachedSecret {
                value: value.to_string(),
                cached_at: chrono::Utc::now(),
                ttl_seconds: 300, // 5 minutes
            },
        );
    }

    async fn invalidate_cache(&self, name: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(name);
    }

    /// Clear all cached secrets
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}
