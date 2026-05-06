//! Org-scoped encrypted secret store.
//!
//! Backed by the `org_secrets` table, encrypted with AES-256-GCM using the
//! server's master key from `OXY_ENCRYPTION_KEY`. Each row carries a
//! `key_version` byte; version 1 uses the current master key.

use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit, Nonce};
use entity::org_secrets;
use entity::prelude::OrgSecrets;
use oxy_shared::errors::OxyError;
use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::db::establish_connection;
use crate::secrets::encryption::get_encryption_key;

const CURRENT_KEY_VERSION: i16 = 1;

#[derive(Debug, Clone)]
pub struct OrgSecretsService;

impl OrgSecretsService {
    /// Upsert a secret under (org_id, name). Returns the secret id.
    pub async fn upsert(org_id: Uuid, name: &str, plaintext: &str) -> Result<Uuid, OxyError> {
        let ciphertext = encrypt(plaintext)?;
        let conn = establish_connection().await?;

        let existing = OrgSecrets::find()
            .filter(org_secrets::Column::OrgId.eq(org_id))
            .filter(org_secrets::Column::Name.eq(name))
            .one(&conn)
            .await
            .map_err(|e| OxyError::DBError(e.to_string()))?;

        if let Some(row) = existing {
            let id = row.id;
            let mut active: org_secrets::ActiveModel = row.into();
            active.ciphertext = ActiveValue::Set(ciphertext);
            active.key_version = ActiveValue::Set(CURRENT_KEY_VERSION);
            active.updated_at = ActiveValue::NotSet;
            active
                .update(&conn)
                .await
                .map_err(|e| OxyError::DBError(e.to_string()))?;
            Ok(id)
        } else {
            let id = Uuid::new_v4();
            org_secrets::ActiveModel {
                id: ActiveValue::Set(id),
                org_id: ActiveValue::Set(org_id),
                name: ActiveValue::Set(name.to_string()),
                ciphertext: ActiveValue::Set(ciphertext),
                key_version: ActiveValue::Set(CURRENT_KEY_VERSION),
                created_at: ActiveValue::NotSet,
                updated_at: ActiveValue::NotSet,
            }
            .insert(&conn)
            .await
            .map_err(|e| OxyError::DBError(e.to_string()))?;
            Ok(id)
        }
    }

    /// Fetch and decrypt by id.
    pub async fn get_by_id(id: Uuid) -> Result<String, OxyError> {
        let conn = establish_connection().await?;
        let row = OrgSecrets::find_by_id(id)
            .one(&conn)
            .await
            .map_err(|e| OxyError::DBError(e.to_string()))?
            .ok_or_else(|| OxyError::DBError(format!("org_secret {id} not found")))?;
        if row.key_version != CURRENT_KEY_VERSION {
            return Err(OxyError::SecretManager(format!(
                "unsupported key_version {} (expected {CURRENT_KEY_VERSION})",
                row.key_version
            )));
        }
        decrypt(&row.ciphertext)
    }

    /// Delete by id.
    pub async fn delete(id: Uuid) -> Result<(), OxyError> {
        let conn = establish_connection().await?;
        OrgSecrets::delete_by_id(id)
            .exec(&conn)
            .await
            .map_err(|e| OxyError::DBError(e.to_string()))?;
        Ok(())
    }
}

fn encrypt(plaintext: &str) -> Result<Vec<u8>, OxyError> {
    let key_bytes = get_encryption_key();
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| OxyError::SecretManager(format!("encrypt failed: {e}")))?;
    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

fn decrypt(blob: &[u8]) -> Result<String, OxyError> {
    if blob.len() < 12 + 16 {
        return Err(OxyError::SecretManager("ciphertext too short".into()));
    }
    let (nonce_bytes, ct) = blob.split_at(12);
    let key_bytes = get_encryption_key();
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ct)
        .map_err(|e| OxyError::SecretManager(format!("decrypt failed: {e}")))?;
    String::from_utf8(plaintext)
        .map_err(|e| OxyError::SecretManager(format!("non-utf8 plaintext: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose};
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    /// Pre-seed a deterministic 32-byte key so tests don't depend on env.
    fn set_test_key() {
        // SAFETY: single-threaded test with ENV_MUTEX serialising the callers.
        unsafe {
            std::env::set_var(
                "OXY_ENCRYPTION_KEY",
                general_purpose::STANDARD.encode([0u8; 32]),
            );
        }
    }

    #[test]
    fn round_trip() {
        let _guard = ENV_MUTEX.lock().unwrap();
        set_test_key();
        let blob = encrypt("hello").unwrap();
        let back = decrypt(&blob).unwrap();
        assert_eq!(back, "hello");
    }

    #[test]
    fn nonce_is_unique_per_call() {
        let _guard = ENV_MUTEX.lock().unwrap();
        set_test_key();
        let a = encrypt("x").unwrap();
        let b = encrypt("x").unwrap();
        assert_ne!(a, b, "nonce must differ between encrypts");
    }

    #[test]
    fn decrypt_rejects_truncated_blob() {
        let _guard = ENV_MUTEX.lock().unwrap();
        set_test_key();
        assert!(decrypt(&[0u8; 5]).is_err());
    }
}
