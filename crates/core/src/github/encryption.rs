use crate::utils::{decrypt_value, encrypt_value, get_encryption_key};
use oxy_shared::errors::OxyError;

/// Secure encryption/decryption for GitHub tokens using AES-GCM
/// This provides authenticated encryption suitable for production use
pub struct TokenEncryption;

impl TokenEncryption {
    /// Encrypt a GitHub token using AES-256-GCM
    /// Returns base64-encoded string containing nonce + ciphertext
    pub fn encrypt_token(token: &str) -> Result<String, OxyError> {
        let key = &get_encryption_key();
        encrypt_value(key, token)
    }

    /// Decrypt a GitHub token using AES-256-GCM
    /// Expects base64-encoded string containing nonce + ciphertext
    pub fn decrypt_token(encrypted_token: &str) -> Result<String, OxyError> {
        let key = &get_encryption_key();
        decrypt_value(key, encrypted_token)
    }

    /// Validate that a token can be encrypted and decrypted correctly
    pub fn validate_token_encryption(token: &str) -> Result<(), OxyError> {
        let encrypted = Self::encrypt_token(token)?;
        let decrypted = Self::decrypt_token(&encrypted)?;

        if token == decrypted {
            Ok(())
        } else {
            Err(OxyError::CryptographyError(
                "Token encryption/decryption validation failed".to_string(),
            ))
        }
    }
}
