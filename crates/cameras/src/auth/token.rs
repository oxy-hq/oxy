//! Token generation + hashing.
//!
//! Format: 32 random bytes encoded as URL-safe base64 (43 chars, no
//! padding). The hash stored in DB is the lowercase hex SHA-256 of the
//! exact plaintext bytes — when verifying, hash the inbound bearer the
//! same way and compare strings.
//!
//! The 8-char prefix is the first 8 chars of the plaintext (NOT the
//! hash). It's safe to log because it's not enough entropy to brute-force.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

/// A freshly minted token. Caller persists `hash` + `prefix`, returns
/// `plaintext` to the edge **once**.
#[derive(Debug, Clone)]
pub struct IssuedToken {
    pub plaintext: String,
    pub hash: String,
    pub prefix: String,
}

/// Generate a new device token. 32 bytes of entropy => 43 chars base64.
pub fn issue() -> IssuedToken {
    let bytes: [u8; 32] = rand::random();
    let plaintext = URL_SAFE_NO_PAD.encode(bytes);
    let prefix: String = plaintext.chars().take(8).collect();
    let hash = hash(&plaintext);
    IssuedToken {
        plaintext,
        hash,
        prefix,
    }
}

/// Hash a token plaintext to its DB-stored form. Used at issue time AND
/// at verify time (compare hashes, never plaintexts).
pub fn hash(plaintext: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plaintext.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_produces_consistent_hash() {
        let t = issue();
        assert_eq!(hash(&t.plaintext), t.hash);
        assert_eq!(t.prefix.len(), 8);
        assert!(t.plaintext.starts_with(&t.prefix));
        // 32 bytes of base64-url-no-pad is 43 chars
        assert_eq!(t.plaintext.len(), 43);
    }

    #[test]
    fn hash_is_deterministic() {
        assert_eq!(hash("hello"), hash("hello"));
        assert_ne!(hash("hello"), hash("world"));
    }

    #[test]
    fn different_tokens_each_call() {
        let a = issue();
        let b = issue();
        assert_ne!(a.plaintext, b.plaintext);
        assert_ne!(a.hash, b.hash);
    }
}
