use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use base64::{Engine as _, engine::general_purpose};
use rand::{Rng, distributions::Alphanumeric};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use short_uuid::ShortUuid;
use uuid::Uuid;

use crate::errors::OxyError;
use entity::api_keys::{
    self, ActiveModel as ApiKeyActiveModel, Entity as ApiKey, Model as ApiKeyModel,
};

pub struct ApiKeyService;

#[derive(Debug, Clone)]
pub struct ApiKeyConfig {
    pub prefix: String,
    pub key_length: usize,
    pub default_expiry_days: Option<u32>,
}

impl Default for ApiKeyConfig {
    fn default() -> Self {
        Self {
            prefix: "oxy_".to_string(),
            key_length: 32,
            default_expiry_days: Some(30),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateApiKeyParams {
    pub user_id: Uuid,
    pub name: String,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub project_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct CreateApiKeyResponse {
    pub id: Uuid,
    pub key: String, // Only returned on creation
    pub name: String,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct ValidatedApiKey {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

// replace magic number with a named constant
const SHORT_UUID_LEN: usize = 22;

impl ApiKeyService {
    fn generate_key(config: &ApiKeyConfig, key_id: Uuid) -> String {
        let mut rng = rand::thread_rng();

        // Use short UUID representation of the key ID for compact format
        let short_uuid = ShortUuid::from_uuid(&key_id);

        // Generate random bytes
        let random_bytes: Vec<u8> = (0..config.key_length)
            .map(|_| rng.sample(Alphanumeric))
            .collect();

        // Create the key with prefix, short UUID (based on key ID), and random suffix
        // Format: oxy_{short_uuid}{random_key}
        let key_suffix = general_purpose::URL_SAFE_NO_PAD.encode(&random_bytes);
        format!("{}{}{}", config.prefix, short_uuid, key_suffix)
    }

    /// Extract the UUID from an API key for fast lookup
    fn extract_key_id(key: &str, config: &ApiKeyConfig) -> Option<Uuid> {
        if !key.starts_with(&config.prefix) {
            return None;
        }
        let after_prefix = &key[config.prefix.len()..];
        if after_prefix.len() >= SHORT_UUID_LEN {
            let short_uuid_str = &after_prefix[..SHORT_UUID_LEN];
            ShortUuid::parse_str(short_uuid_str)
                .ok()
                .map(|su| su.to_uuid())
        } else {
            None
        }
    }

    /// Hash an API key using Argon2
    fn hash_key(key: &str) -> Result<String, OxyError> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        let password_hash = argon2.hash_password(key.as_bytes(), &salt).map_err(|err| {
            OxyError::ConfigurationError(format!("Failed to hash API key: {err}"))
        })?;

        Ok(password_hash.to_string())
    }

    /// Verify an API key against its hash
    fn verify_key(key: &str, hash: &str) -> Result<bool, OxyError> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|err| OxyError::ConfigurationError(format!("Invalid password hash: {err}")))?;

        let argon2 = Argon2::default();
        Ok(argon2.verify_password(key.as_bytes(), &parsed_hash).is_ok())
    }

    /// Create a new API key for a user
    pub async fn create_api_key(
        db: &DatabaseConnection,
        request: CreateApiKeyParams,
        config: &ApiKeyConfig,
    ) -> Result<CreateApiKeyResponse, OxyError> {
        // Create the database record ID first
        let id = Uuid::new_v4();

        // Generate the API key using the ID
        let key = Self::generate_key(config, id);
        let key_hash = Self::hash_key(&key)?;

        // Set expiry date if specified in config
        let expires_at = request.expires_at.or_else(|| {
            config
                .default_expiry_days
                .map(|days| chrono::Utc::now() + chrono::Duration::days(days as i64))
        });

        let now = chrono::Utc::now();

        let active_model = ApiKeyActiveModel {
            id: Set(id),
            user_id: Set(request.user_id),
            key_hash: Set(key_hash),
            name: Set(request.name.clone()),
            expires_at: Set(expires_at.map(|dt| dt.into())),
            last_used_at: Set(None),
            created_at: Set(now.into()),
            updated_at: Set(now.into()),
            is_active: Set(true),
            project_id: Set(request.project_id),
        };

        // Save to database
        let api_key_model = ApiKey::insert(active_model)
            .exec_with_returning(db)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to create API key: {err}")))?;

        Ok(CreateApiKeyResponse {
            id: api_key_model.id,
            key, // Return the plain key only on creation
            name: api_key_model.name,
            expires_at,
            created_at: now,
        })
    }

    /// Validate an API key and return user information (optimized with key ID lookup)
    pub async fn validate_api_key(
        db: &DatabaseConnection,
        key: &str,
        config: &ApiKeyConfig,
    ) -> Result<Option<ValidatedApiKey>, OxyError> {
        // Extract the key ID from the key for fast lookup
        let key_id = Self::extract_key_id(key, config)
            .ok_or_else(|| OxyError::ValidationError("Invalid API key format".to_string()))?;

        // Lookup by specific key ID
        let api_key = ApiKey::find_by_id(key_id)
            .filter(api_keys::Column::IsActive.eq(true))
            .one(db)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to fetch API key: {err}")))?;

        let Some(api_key_model) = api_key else {
            return Ok(None);
        };

        // Check if expired
        if api_key_model.is_expired() {
            return Ok(None);
        }

        // Verify the key against the hash
        if !Self::verify_key(key, &api_key_model.key_hash)? {
            return Ok(None);
        }

        // decide whether to bump last_used_at
        let should_update =
            Self::should_update_timestamp(api_key_model.last_used_at.map(|dt| dt.into()));

        let validated_key = ValidatedApiKey {
            id: api_key_model.id,
            user_id: api_key_model.user_id,
            name: api_key_model.name,
            last_used_at: api_key_model.last_used_at.map(|dt| dt.into()),
        };

        if should_update {
            Self::spawn_update_last_used(db.clone(), api_key_model.id);
        }

        Ok(Some(validated_key))
    }

    /// Revoke an API key (soft delete)
    pub async fn revoke_api_key(
        db: &DatabaseConnection,
        key_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), OxyError> {
        let api_key = ApiKey::find_by_id(key_id)
            .filter(api_keys::Column::UserId.eq(user_id))
            .one(db)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to find API key: {err}")))?;

        let Some(api_key_model) = api_key else {
            return Err(OxyError::ValidationError("API key not found".to_string()));
        };

        let mut active_model: ApiKeyActiveModel = api_key_model.into();
        active_model.is_active = Set(false);
        active_model.updated_at = Set(chrono::Utc::now().into());

        ApiKey::update(active_model)
            .exec(db)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to revoke API key: {err}")))?;

        Ok(())
    }

    /// List all active API keys for a user (without the actual key values)
    pub async fn list_user_api_keys(
        db: &DatabaseConnection,
        user_id: Uuid,
    ) -> Result<Vec<ApiKeyModel>, OxyError> {
        let api_keys = ApiKey::find()
            .filter(api_keys::Column::UserId.eq(user_id))
            .filter(api_keys::Column::IsActive.eq(true))
            .all(db)
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to fetch user API keys: {err}")))?;

        Ok(api_keys)
    }

    fn should_update_timestamp(last_used: Option<chrono::DateTime<chrono::Utc>>) -> bool {
        match last_used {
            None => true,
            Some(ts) => ts < chrono::Utc::now() - chrono::Duration::minutes(10),
        }
    }

    fn spawn_update_last_used(db: DatabaseConnection, key_id: Uuid) {
        tokio::spawn(async move {
            let now = chrono::Utc::now();
            let active_model = ApiKeyActiveModel {
                id: Set(key_id),
                last_used_at: Set(Some(now.into())),
                updated_at: Set(now.into()),
                ..Default::default()
            };
            if let Err(err) = ApiKey::update(active_model).exec(&db).await {
                eprintln!("Failed to update last_used_at for API key {key_id}: {err}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_generate_key() {
        let config = ApiKeyConfig::default();
        let key_id = Uuid::new_v4();
        let key = ApiKeyService::generate_key(&config, key_id);

        assert!(key.starts_with("oxy_"));
        assert!(key.len() > config.prefix.len());
    }

    #[test]
    fn test_extract_key_id() {
        let config = ApiKeyConfig::default();
        let key_id = Uuid::new_v4();
        let key = ApiKeyService::generate_key(&config, key_id);

        let extracted_id = ApiKeyService::extract_key_id(&key, &config);
        assert_eq!(extracted_id, Some(key_id));
    }

    #[test]
    fn test_hash_and_verify_key() {
        let key = "test_key_123";
        let hash = ApiKeyService::hash_key(key).unwrap();

        assert!(ApiKeyService::verify_key(key, &hash).unwrap());
        assert!(!ApiKeyService::verify_key("wrong_key", &hash).unwrap());
    }

    #[test]
    fn test_extract_key_id_invalid_prefix() {
        let config = ApiKeyConfig::default();
        let key_id = Uuid::new_v4();
        let mut key = ApiKeyService::generate_key(&config, key_id);
        // corrupt the prefix
        key = key.replacen(&config.prefix, "bad_", 1);
        assert_eq!(ApiKeyService::extract_key_id(&key, &config), None);
    }

    #[test]
    fn test_extract_key_id_short() {
        let config = ApiKeyConfig::default();
        // too short to contain a SHORT_UUID_LEN
        let key = format!("{}{}", config.prefix, "short");
        assert_eq!(ApiKeyService::extract_key_id(&key, &config), None);
    }

    #[test]
    fn test_should_update_timestamp_none() {
        assert!(ApiKeyService::should_update_timestamp(None));
    }

    #[test]
    fn test_should_update_timestamp_recent() {
        let recent = Utc::now();
        assert!(!ApiKeyService::should_update_timestamp(Some(recent)));
    }

    #[test]
    fn test_should_update_timestamp_old() {
        let old = Utc::now() - Duration::minutes(20);
        assert!(ApiKeyService::should_update_timestamp(Some(old)));
    }
}
