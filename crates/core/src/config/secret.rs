use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretConfig {
    encryption_key: Option<String>,
}

impl SecretConfig {
    /// Get the encryption key from environment variable or config
    pub fn get_encryption_key(&self) -> Option<String> {
        std::env::var("OXY_ENCRYPTION_KEY")
            .ok()
            .or_else(|| self.encryption_key.clone())
    }

    /// Validate the secret configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.encryption_key.is_none() {
            return Err("Encryption key must be set".to_string());
        }
        Ok(())
    }
}
