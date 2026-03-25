use uuid::Uuid;

use crate::adapters::secrets::{SecretsDatabaseStorage, environment::SecretsEnvironmentStorage};
use oxy_shared::errors::OxyError;

#[enum_dispatch::enum_dispatch]
#[allow(async_fn_in_trait)]
pub trait SecretsStorage {
    async fn resolve_secret(&self, secret_name: &str) -> Result<Option<String>, OxyError>;
    async fn create_secret(
        &self,
        secret_name: &str,
        secret_value: &str,
        created_by: Uuid,
    ) -> Result<(), OxyError>;
    async fn remove_secret(&self, secret_name: &str) -> Result<(), OxyError>;
}

#[enum_dispatch::enum_dispatch(SecretsStorage)]
#[derive(Debug, Clone)]
pub enum SecretsStorageImpl {
    DatabaseStorage(SecretsDatabaseStorage),
    EnvironmentStorage(SecretsEnvironmentStorage),
    FallbackStorage(SecretsFallbackStorage),
}

/// Checks the database first, then falls back to environment variables.
///
/// This is used in local mode so that a secret stored in the DB immediately
/// overrides the corresponding env var — no restart required.
#[derive(Debug, Clone)]
pub struct SecretsFallbackStorage {
    db: SecretsDatabaseStorage,
    env: SecretsEnvironmentStorage,
}

impl SecretsFallbackStorage {
    pub fn new(db: SecretsDatabaseStorage) -> Self {
        Self {
            db,
            env: SecretsEnvironmentStorage,
        }
    }
}

impl SecretsStorage for SecretsFallbackStorage {
    async fn resolve_secret(&self, secret_name: &str) -> Result<Option<String>, OxyError> {
        // DB takes precedence — enables hot-reload / override of env vars
        if let Some(value) = self.db.resolve_secret(secret_name).await? {
            return Ok(Some(value));
        }
        self.env.resolve_secret(secret_name).await
    }

    async fn create_secret(
        &self,
        secret_name: &str,
        secret_value: &str,
        created_by: Uuid,
    ) -> Result<(), OxyError> {
        self.db
            .create_secret(secret_name, secret_value, created_by)
            .await
    }

    async fn remove_secret(&self, secret_name: &str) -> Result<(), OxyError> {
        self.db.remove_secret(secret_name).await
    }
}
