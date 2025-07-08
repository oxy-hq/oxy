use sea_orm::DatabaseConnection;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::errors::OxyError;
use crate::service::secret_manager::SecretManagerService;

pub struct SecretResolverService {
    secret_manager: SecretManagerService,
    env_cache: Arc<RwLock<HashMap<String, Option<String>>>>,
}

#[derive(Debug, Clone)]
pub struct SecretResolutionResult {
    pub value: String,
    pub source: SecretSource,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SecretSource {
    SecretManager,
    Environment,
    Config,
}

impl Default for SecretResolverService {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretResolverService {
    pub fn new() -> Self {
        Self {
            secret_manager: SecretManagerService::new(),
            env_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Resolve a secret with hierarchical fallback:
    /// 1. Check secret manager
    /// 2. Check environment variables
    pub async fn resolve_secret(
        &self,
        secret_name: &str,
    ) -> Result<Option<SecretResolutionResult>, OxyError> {
        // 1. Try secret manager first
        if let Some(value) = self.secret_manager.get_secret(secret_name).await {
            return Ok(Some(SecretResolutionResult {
                value,
                source: SecretSource::SecretManager,
            }));
        }

        // 2. Try environment variable
        if let Some(value) = self.get_env_var(secret_name).await {
            return Ok(Some(SecretResolutionResult {
                value,
                source: SecretSource::Environment,
            }));
        }

        Ok(None)
    }

    /// Get environment variable with caching
    async fn get_env_var(&self, var_name: &str) -> Option<String> {
        // Check cache first
        {
            let cache = self.env_cache.read().await;
            if let Some(cached_value) = cache.get(var_name) {
                return cached_value.clone();
            }
        }

        // Get from environment and cache the result
        let value = std::env::var(var_name).ok();
        {
            let mut cache = self.env_cache.write().await;
            cache.insert(var_name.to_string(), value.clone());
        }

        value
    }

    /// Clear environment variable cache
    pub async fn clear_env_cache(&self) {
        let mut cache = self.env_cache.write().await;
        cache.clear();
    }

    /// Clear all caches
    pub async fn clear_all_caches(&self) {
        self.secret_manager.clear_cache().await;
        self.clear_env_cache().await;
    }

    /// Get the secret manager service for direct access
    pub fn secret_manager(&self) -> &SecretManagerService {
        &self.secret_manager
    }
}

// Helper trait for common secret resolution patterns
#[async_trait::async_trait]
pub trait SecretResolver {
    async fn resolve_password(
        &self,
        resolver: &SecretResolverService,
        db: &DatabaseConnection,
    ) -> Result<Option<String>, OxyError>;

    async fn resolve_api_key(
        &self,
        resolver: &SecretResolverService,
        db: &DatabaseConnection,
    ) -> Result<Option<String>, OxyError>;
}
