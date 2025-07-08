use axum::http::HeaderMap;
use std::sync::Arc;

use crate::db::client::establish_connection;
use crate::errors::OxyError;
use crate::service::api_key::{ApiKeyConfig, ApiKeyService, ValidatedApiKey};
use crate::{
    auth::{authenticator::Authenticator, types::Identity},
    config::auth::ApiKeyAuth,
};

/// API Key Authenticator for authenticating requests using API keys
pub struct ApiKeyAuthenticator {
    db_connection: Option<Arc<sea_orm::DatabaseConnection>>,
    api_key_header: String,
}

impl ApiKeyAuthenticator {
    pub fn from_config(config: ApiKeyAuth) -> Self {
        Self {
            db_connection: None,
            api_key_header: config.header,
        }
    }

    /// Extract API key from X-API-Key header only
    fn extract_api_key_from_headers(&self, headers: &HeaderMap) -> Option<String> {
        // Extract only from X-API-Key header
        if let Some(api_key_header) = headers.get(self.api_key_header.as_str()) {
            if let Ok(key_str) = api_key_header.to_str() {
                return Some(key_str.trim().to_string());
            }
        }

        None
    }

    /// Get or establish database connection
    async fn get_db_connection(&self) -> Result<Arc<sea_orm::DatabaseConnection>, OxyError> {
        match &self.db_connection {
            Some(conn) => Ok(Arc::clone(conn)),
            None => {
                let conn = establish_connection().await?;
                Ok(Arc::new(conn))
            }
        }
    }

    /// Validate API key and convert to Identity
    pub async fn validate_api_key(
        &self,
        key: &str,
    ) -> Result<(Identity, ValidatedApiKey), OxyError> {
        let db = self.get_db_connection().await?;
        let config = ApiKeyConfig::default();

        let validated_key = ApiKeyService::validate_api_key(&db, key, &config)
            .await?
            .ok_or_else(|| OxyError::AuthenticationError("Invalid API key".to_string()))?;

        // Convert validated API key to Identity
        let identity = Identity {
            idp_id: Some(validated_key.id.to_string()),
            email: format!("api-key-{}", validated_key.user_id), // Placeholder email for API key users
            name: Some(validated_key.name.clone()),
            picture: None,
        };

        Ok((identity, validated_key))
    }
}

impl Authenticator for ApiKeyAuthenticator {
    type Error = OxyError;

    async fn authenticate(&self, headers: &HeaderMap) -> Result<Identity, Self::Error> {
        let key = self.extract_api_key_from_headers(headers).ok_or_else(|| {
            OxyError::AuthenticationError("No API key found in headers".to_string())
        })?;

        let (identity, _validated_key) = self.validate_api_key(&key).await?;
        Ok(identity)
    }
}
