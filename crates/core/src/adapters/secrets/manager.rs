use crate::{
    adapters::secrets::{
        SecretsDatabaseStorage, SecretsStorage, environment::SecretsEnvironmentStorage,
        storage::SecretsStorageImpl,
    },
    errors::OxyError,
    service::secret_manager::SecretManagerService,
};

#[derive(Debug, Clone)]
pub struct SecretsManager {
    storage: SecretsStorageImpl,
}

impl SecretsManager {
    pub fn from_environment() -> Result<Self, OxyError> {
        Ok(SecretsManager {
            storage: SecretsStorageImpl::EnvironmentStorage(SecretsEnvironmentStorage {}),
        })
    }

    pub fn from_database(secret_manager: SecretManagerService) -> Result<Self, OxyError> {
        let secrets_database_storage = SecretsDatabaseStorage::new(secret_manager);
        Ok(SecretsManager {
            storage: SecretsStorageImpl::DatabaseStorage(secrets_database_storage),
        })
    }

    pub async fn resolve_secret(&self, secret_name: &str) -> Result<Option<String>, OxyError> {
        self.storage.resolve_secret(secret_name).await
    }
}
