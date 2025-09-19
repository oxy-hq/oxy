use crate::{
    adapters::secrets::SecretsStorage, errors::OxyError,
    service::secret_manager::SecretManagerService,
};

#[derive(Debug, Clone)]
pub struct SecretsDatabaseStorage {
    secret_manager: SecretManagerService,
}

impl SecretsDatabaseStorage {
    pub fn new(secret_manager: SecretManagerService) -> Self {
        SecretsDatabaseStorage { secret_manager }
    }
}

impl SecretsStorage for SecretsDatabaseStorage {
    async fn resolve_secret(&self, secret_name: &str) -> Result<Option<String>, OxyError> {
        Ok(self.secret_manager.get_secret(secret_name).await)
    }
}
