use crate::{
    adapters::secrets::SecretsStorage,
    db::client::establish_connection,
    errors::OxyError,
    service::secret_manager::{CreateSecretParams, SecretManagerService},
};
use uuid::Uuid;

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

    async fn create_secret(
        &self,
        secret_name: &str,
        secret_value: &str,
        created_by: Uuid,
    ) -> Result<(), OxyError> {
        let db = establish_connection()
            .await
            .map_err(|e| OxyError::Database(format!("Failed to establish connection: {}", e)))?;

        let params = CreateSecretParams {
            name: secret_name.to_string(),
            value: secret_value.to_string(),
            description: None,
            created_by, // System-created secret
        };

        self.secret_manager.create_secret(&db, params).await?;
        Ok(())
    }

    async fn remove_secret(&self, secret_name: &str) -> Result<(), OxyError> {
        let db = establish_connection()
            .await
            .map_err(|e| OxyError::Database(format!("Failed to establish connection: {}", e)))?;

        self.secret_manager.delete_secret(&db, secret_name).await
    }
}
