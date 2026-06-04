//! At-rest sealing of camera-domain secrets.
//!
//! Thin wrapper over [`oxy_platform::secrets::envelope`] that adds a
//! 1-byte version prefix. The prefix lets the backfill migration
//! detect "already sealed" rows and skip them, so the migration is
//! idempotent under partial replays.
//!
//! Wire format: `version_byte || envelope_blob` where
//! `envelope_blob = nonce(12) || ciphertext`.
//!
//! Used by `device_registry.device_secret` today. The same helper
//! is the obvious home for any future at-rest secret in this
//! domain (RTSP credential blobs, UniFi API keys held server-side).

use oxy_platform::secrets::envelope;
use oxy_shared::errors::OxyError;

/// Marker for AES-256-GCM sealed payload. Bump (e.g. to `0x02`)
/// if we ever change KDF or AEAD — the open path can fork on
/// the byte before delegating.
const SEAL_VERSION: u8 = 0x01;

/// Seal `plaintext` under the platform master key, prefixed with
/// a version byte. Returns `[version, nonce(12), ciphertext...]`.
pub fn seal(plaintext: &[u8]) -> Result<Vec<u8>, OxyError> {
    let inner = envelope::seal(plaintext)?;
    let mut out = Vec::with_capacity(1 + inner.len());
    out.push(SEAL_VERSION);
    out.extend(inner);
    Ok(out)
}

/// Open a blob produced by [`seal`]. Errors if the version byte
/// doesn't match — that's the signal the caller should treat the
/// blob as legacy plaintext (or fail, depending on context).
pub fn open(blob: &[u8]) -> Result<Vec<u8>, OxyError> {
    match blob.first() {
        Some(&SEAL_VERSION) => envelope::open(&blob[1..]),
        Some(other) => Err(OxyError::SecretManager(format!(
            "unknown seal version byte: 0x{other:02x}"
        ))),
        None => Err(OxyError::SecretManager("empty sealed blob".into())),
    }
}

/// Whether the blob is already sealed under the current version.
/// Used by the backfill migration to skip rows it's already
/// processed on a partial replay.
pub fn looks_sealed(blob: &[u8]) -> bool {
    blob.first() == Some(&SEAL_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose};
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    fn set_test_key() {
        // SAFETY: tests holding ENV_MUTEX serialize set_var.
        unsafe {
            std::env::set_var(
                "OXY_ENCRYPTION_KEY",
                general_purpose::STANDARD.encode([0u8; 32]),
            );
        }
    }

    #[test]
    fn round_trip() {
        let _g = ENV_MUTEX.lock().unwrap();
        set_test_key();
        let blob = seal(b"device-secret-32-bytes-go-here..").unwrap();
        assert_eq!(open(&blob).unwrap(), b"device-secret-32-bytes-go-here..");
    }

    #[test]
    fn looks_sealed_is_true_after_seal() {
        let _g = ENV_MUTEX.lock().unwrap();
        set_test_key();
        let blob = seal(b"x").unwrap();
        assert!(looks_sealed(&blob));
        assert!(!looks_sealed(b"raw-plaintext"));
    }

    #[test]
    fn open_rejects_plaintext() {
        let _g = ENV_MUTEX.lock().unwrap();
        set_test_key();
        // Raw plaintext shouldn't open as sealed.
        let result = open(b"unsealed bytes here");
        assert!(result.is_err());
    }
}
