use crate::{adapters::secrets::SecretsStorage, errors::OxyError};

#[derive(Debug, Clone)]
pub struct SecretsEnvironmentStorage;

impl SecretsStorage for SecretsEnvironmentStorage {
    async fn resolve_secret(&self, secret_name: &str) -> Result<Option<String>, OxyError> {
        Ok(std::env::var(secret_name).ok())
    }
}
