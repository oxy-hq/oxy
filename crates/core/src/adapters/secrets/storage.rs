use crate::{
    adapters::secrets::{SecretsDatabaseStorage, environment::SecretsEnvironmentStorage},
    errors::OxyError,
};

#[enum_dispatch::enum_dispatch]
pub trait SecretsStorage {
    async fn resolve_secret(&self, secret_name: &str) -> Result<Option<String>, OxyError>;
}

#[enum_dispatch::enum_dispatch(SecretsStorage)]
#[derive(Debug, Clone)]
pub enum SecretsStorageImpl {
    DatabaseStorage(SecretsDatabaseStorage),
    EnvironmentStorage(SecretsEnvironmentStorage),
}
