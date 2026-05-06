//! AES-256 master key loader.
//!
//! Looks up the key in this order:
//! 1. `OXY_ENCRYPTION_KEY` env var (base64-encoded 32 bytes).
//! 2. `<state-dir>/encryption_key.txt` (created on first call when the env
//!    var is unset — convenient for local dev, **not safe for production**).
//! 3. If neither exists, generates a fresh key and writes it to the file.
//!
//! The master key is consumed by [`crate::secrets::OrgSecretsService`] for
//! per-org secret encryption and by other oxy modules that encrypt their
//! own values (GitHub OAuth tokens, the legacy `_var` secret manager).

use std::fs;
use std::path::PathBuf;

use aes_gcm::aead::OsRng;
use aes_gcm::{Aes256Gcm, KeyInit};
use base64::{Engine as _, engine::general_purpose, engine::general_purpose::STANDARD as BASE64};
use oxy_shared::errors::OxyError;

const OXY_ENCRYPTION_KEY_VAR: &str = "OXY_ENCRYPTION_KEY";

/// Look up oxy's state directory.
///
/// Mirrors `oxy::state_dir::get_state_dir` — duplicated here (rather than
/// imported) because that function lives in the `oxy` crate, which depends
/// on `oxy-platform`. Pulling it in would invert the dependency direction.
fn get_state_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("OXY_STATE_DIR") {
        return PathBuf::from(dir);
    }
    dirs::data_local_dir()
        .map(|d| d.join("oxy"))
        .unwrap_or_else(|| PathBuf::from(".oxy_state"))
}

fn get_key_file_path() -> PathBuf {
    get_state_dir().join("encryption_key.txt")
}

fn decode_key_from_string(key_str: &str) -> [u8; 32] {
    let decoded = general_purpose::STANDARD
        .decode(key_str)
        .map_err(|e| OxyError::SecretManager(format!("Invalid encryption key format: {e}")))
        .expect("Failed to decode encryption key");

    if decoded.len() != 32 {
        panic!(
            "Invalid encryption key length: expected 32 bytes, got {}",
            decoded.len()
        );
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&decoded);
    key
}

/// Get the encryption key from environment variable.
/// Falls back to a development key for development (NOT secure for production).
pub fn get_encryption_key() -> [u8; 32] {
    // First try environment variable
    if let Ok(key_str) = std::env::var(OXY_ENCRYPTION_KEY_VAR) {
        return decode_key_from_string(&key_str);
    }

    // Try loading from file
    let key_file_path = get_key_file_path();
    if let Ok(key_str) = fs::read_to_string(&key_file_path) {
        let key_str = key_str.trim();
        if !key_str.is_empty() {
            tracing::info!("Loading encryption key from file: {:?}", key_file_path);
            return decode_key_from_string(key_str);
        }
    }

    // Generate a new key and save it to file
    let key = Aes256Gcm::generate_key(&mut OsRng);

    // Ensure directory exists
    if let Some(parent) = key_file_path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        tracing::error!("Failed to create directory for encryption key: {}", e);
    }
    // Encode key as base64 string
    let key_string = BASE64.encode(key);

    // Save key to file
    if let Err(e) = fs::write(&key_file_path, &key_string) {
        tracing::error!("Failed to save encryption key to file: {}", e);
    } else {
        tracing::info!(
            "Generated new encryption key and saved to: {:?}",
            key_file_path
        );
    }

    tracing::warn!(
        "No encryption key found. Generated new key and saved to: {:?}",
        key_file_path
    );
    key.into()
}
