use uuid::Uuid;

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

    pub async fn create_secret(
        &self,
        secret_name: &str,
        secret_value: &str,
        created_by: Uuid,
    ) -> Result<(), OxyError> {
        self.storage
            .create_secret(secret_name, secret_value, created_by)
            .await
    }

    pub async fn remove_secret(&self, secret_name: &str) -> Result<(), OxyError> {
        self.storage.remove_secret(secret_name).await
    }

    /// Resolve a config value from either a direct value or an environment variable.
    ///
    /// This is a common pattern used throughout the codebase for config fields that
    /// can be specified directly or via an environment variable reference.
    ///
    /// # Arguments
    /// * `direct_value` - The direct value if specified (e.g., `password` field)
    /// * `var_name` - The environment variable name if specified (e.g., `password_var` field)
    /// * `field_name` - Human-readable field name for error messages
    /// * `default` - Optional default value if neither direct nor var is specified
    ///
    /// # Returns
    /// * `Ok(String)` - The resolved value
    /// * `Err(OxyError::SecretNotFound)` - If var_name was specified but the secret wasn't found
    /// * `Err(OxyError::ConfigurationError)` - If no value could be resolved and no default provided
    pub async fn resolve_config_value(
        &self,
        direct_value: Option<&str>,
        var_name: Option<&str>,
        field_name: &str,
        default: Option<&str>,
    ) -> Result<String, OxyError> {
        // Try direct value first
        if let Some(value) = direct_value
            && !value.is_empty()
        {
            return Ok(value.to_string());
        }

        // Try resolving from environment variable
        if let Some(var) = var_name
            && !var.is_empty()
        {
            let resolved = self.resolve_secret(var).await?;
            if let Some(res) = resolved {
                return Ok(res);
            }
            return Err(OxyError::SecretNotFound(Some(var.to_string())));
        }

        // Fall back to default if provided
        if let Some(def) = default {
            return Ok(def.to_string());
        }

        // No value found
        Err(OxyError::ConfigurationError(format!(
            "{} or {}_var must be specified",
            field_name, field_name
        )))
    }
}
