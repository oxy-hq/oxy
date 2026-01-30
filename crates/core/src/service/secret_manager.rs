use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use base64::{Engine as _, engine::general_purpose};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use secrecy::{ExposeSecret, SecretString};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use entity::secrets::{self, ActiveModel as SecretActiveModel, Entity as Secret};
use oxy_shared::errors::OxyError;

use crate::{database::client::establish_connection, utils::get_encryption_key};

/// A managed secret that holds a reference to a secret key variable.
///
/// This type can be deserialized directly from a string in YAML/JSON configs:
/// ```yaml
/// secret: AWS_S3_SECRET
/// ```
///
/// The actual secret value is retrieved via the `expose` method which
/// looks up the secret from either `SecretManagerService` (database) or
/// `SecretsManager` (supports both env vars and database).
#[derive(Clone)]
pub struct ManagedSecret {
    key_var: String,
}

impl ManagedSecret {
    /// Create a new ManagedSecret with the given key variable name.
    pub fn new(key_var: impl Into<String>) -> Self {
        Self {
            key_var: key_var.into(),
        }
    }

    /// Get the key variable name.
    pub fn key_var(&self) -> &str {
        &self.key_var
    }

    /// Expose the secret value by looking it up from the SecretManagerService.
    ///
    /// Returns the secret wrapped in a `SecretString` for safe handling.
    pub async fn expose(
        &self,
        secret_manager: &SecretManagerService,
    ) -> Result<SecretString, OxyError> {
        secret_manager
            .get_secret(&self.key_var)
            .await
            .map(SecretString::from)
            .ok_or_else(|| OxyError::SecretManager(format!("Secret '{}' not found", self.key_var)))
    }

    /// Expose the secret value as a plain string using SecretManagerService.
    ///
    /// Use this when you need the raw string value. Prefer `expose()` when possible
    /// to keep the secret wrapped in `SecretString`.
    pub async fn expose_str(
        &self,
        secret_manager: &SecretManagerService,
    ) -> Result<String, OxyError> {
        let secret = self.expose(secret_manager).await?;
        Ok(secret.expose_secret().to_string())
    }

    /// Expose the secret value using the SecretsManager adapter.
    ///
    /// This is the preferred method as it supports both environment variables
    /// and database-backed secrets.
    pub async fn expose_with_adapter(
        &self,
        secrets_manager: &crate::adapters::secrets::SecretsManager,
    ) -> Result<SecretString, OxyError> {
        secrets_manager
            .resolve_secret(&self.key_var)
            .await?
            .map(SecretString::from)
            .ok_or_else(|| OxyError::SecretManager(format!("Secret '{}' not found", self.key_var)))
    }

    /// Expose the secret value as a plain string using the SecretsManager adapter.
    pub async fn expose_str_with_adapter(
        &self,
        secrets_manager: &crate::adapters::secrets::SecretsManager,
    ) -> Result<String, OxyError> {
        secrets_manager
            .resolve_secret(&self.key_var)
            .await?
            .ok_or_else(|| OxyError::SecretManager(format!("Secret '{}' not found", self.key_var)))
    }
}

impl fmt::Debug for ManagedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Don't expose the key_var in debug output for security
        f.debug_struct("ManagedSecret")
            .field("key_var", &"[REDACTED]")
            .finish()
    }
}

impl fmt::Display for ManagedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ManagedSecret({})", self.key_var)
    }
}

// Deserialize directly from a string
impl<'de> serde::Deserialize<'de> for ManagedSecret {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let key_var = String::deserialize(deserializer)?;
        Ok(ManagedSecret { key_var })
    }
}

// Serialize as a string
impl serde::Serialize for ManagedSecret {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.key_var)
    }
}

// Implement JsonSchema for schemars
impl schemars::JsonSchema for ManagedSecret {
    fn schema_name() -> String {
        "ManagedSecret".to_string()
    }

    fn json_schema(generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        // ManagedSecret is serialized as a string (the key_var name)
        <String as schemars::JsonSchema>::json_schema(generator)
    }
}

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
            project_id,
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
    pub async fn create_secret<C>(
        &self,
        db: &C,
        params: CreateSecretParams,
    ) -> Result<SecretInfo, OxyError>
    where
        C: sea_orm::ConnectionTrait,
    {
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
    pub async fn list_secrets<C>(&self, db: &C) -> Result<Vec<SecretInfo>, OxyError>
    where
        C: sea_orm::ConnectionTrait,
    {
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
    pub async fn update_secret<C>(
        &self,
        db: &C,
        name: &str,
        params: UpdateSecretParams,
    ) -> Result<SecretInfo, OxyError>
    where
        C: sea_orm::ConnectionTrait,
    {
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
    pub async fn delete_secret<C>(&self, db: &C, name: &str) -> Result<(), OxyError>
    where
        C: sea_orm::ConnectionTrait,
    {
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
