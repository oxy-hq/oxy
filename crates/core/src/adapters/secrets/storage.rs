use uuid::Uuid;

use crate::adapters::secrets::{SecretsDatabaseStorage, environment::SecretsEnvironmentStorage};
use oxy_shared::errors::OxyError;

#[enum_dispatch::enum_dispatch]
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
}
