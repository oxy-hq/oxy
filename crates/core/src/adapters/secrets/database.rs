use crate::{
    adapters::secrets::SecretsStorage,
    database::client::establish_connection,
    service::secret_manager::{CreateSecretParams, SecretManagerService, UpdateSecretParams},
};
use oxy_shared::errors::OxyError;
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

    async fn upsert_secret(
        &self,
        secret_name: &str,
        secret_value: &str,
        updated_by: Uuid,
    ) -> Result<(), OxyError> {
        let db = establish_connection()
            .await
            .map_err(|e| OxyError::Database(format!("Failed to establish connection: {}", e)))?;

        // Update-first: a single atomic UPDATE when the row exists. Only
        // fall back to create when it's genuinely absent — `update_secret`
        // signals not-found as `OxyError::SecretManager`.
        let update = self
            .secret_manager
            .update_secret(
                &db,
                secret_name,
                UpdateSecretParams {
                    value: Some(secret_value.to_string()),
                    description: None,
                    updated_by,
                },
            )
            .await;
        match update {
            Ok(_) => Ok(()),
            Err(OxyError::SecretManager(_)) => {
                self.secret_manager
                    .create_secret(
                        &db,
                        CreateSecretParams {
                            name: secret_name.to_string(),
                            value: secret_value.to_string(),
                            description: None,
                            created_by: updated_by,
                        },
                    )
                    .await?;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    async fn remove_secret(&self, secret_name: &str) -> Result<(), OxyError> {
        let db = establish_connection()
            .await
            .map_err(|e| OxyError::Database(format!("Failed to establish connection: {}", e)))?;

        self.secret_manager.delete_secret(&db, secret_name).await
    }
}
