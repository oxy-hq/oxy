//! Blob (asset) storage abstraction.
//!
//! Oxy persists generated assets — charts today, potentially more in the future
//! (exported images, attachments, etc.) — as opaque blobs keyed by filename.
//! Historically this was plain filesystem access rooted under the project state
//! directory. This module adds an abstraction so those blobs can instead live
//! in S3 (or any future backend), which is required for integrations that need
//! a publicly reachable URL — e.g. Slack unfurling a chart image.
//!
//! The abstraction is deliberately narrow: put, get, and an optional
//! `public_url`. Local disk returns `None` for `public_url`; S3 returns the
//! object URL (or a configured CDN base).

use async_trait::async_trait;
use oxy_shared::errors::OxyError;
use std::sync::Arc;

mod chart_image;
mod local;
mod s3;

pub use chart_image::{
    BlobStorageChartImagePublisher, ChartImagePublisher, ChartImageRenderer,
    SharedChartImagePublisher, SharedChartImageRenderer,
};
pub use local::LocalBlobStorage;
pub use s3::{S3BlobStorage, S3BlobStorageConfig};

/// Storage for opaque byte blobs keyed by name.
///
/// Implementations must be safe to share across async tasks. Keys are treated
/// as relative paths — implementations may reject keys containing `..` or
/// absolute path prefixes.
#[async_trait]
pub trait BlobStorage: Send + Sync + std::fmt::Debug {
    /// Store `data` under `key`. Overwrites any existing blob at that key.
    async fn put(&self, key: &str, data: Vec<u8>, content_type: &str) -> Result<(), OxyError>;

    /// Fetch the blob stored at `key`.
    async fn get(&self, key: &str) -> Result<Vec<u8>, OxyError>;

    /// Return a publicly reachable URL for `key`, if the backend exposes one.
    ///
    /// Returns `Ok(None)` for backends that have no notion of a public URL
    /// (local disk). Returning a URL does not guarantee the object is actually
    /// readable without credentials — that depends on bucket policy.
    async fn public_url(&self, key: &str) -> Result<Option<String>, OxyError>;
}

/// Convenience alias — blob storage is always shared behind an Arc.
pub type SharedBlobStorage = Arc<dyn BlobStorage>;

fn validate_key(key: &str) -> Result<(), OxyError> {
    if key.is_empty() {
        return Err(OxyError::RuntimeError(
            "blob storage key must not be empty".to_string(),
        ));
    }
    // `contains("..")` would also reject legitimate filenames like
    // `chart..v2.png`; split on `/` and only reject a literal `..` segment.
    let has_parent_segment = key.split('/').any(|seg| seg == "..");
    if key.starts_with('/') || has_parent_segment || key.contains('\0') {
        return Err(OxyError::RuntimeError(format!(
            "invalid blob storage key: {key:?}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_key_accepts_simple_names() {
        for key in [
            "chart.json",
            "abc/def.png",
            "a-b_c.1.json",
            // Double dots inside a segment are fine — only a literal `..`
            // segment is a traversal.
            "chart..v2.png",
            "a/b/c..d.png",
        ] {
            assert!(validate_key(key).is_ok(), "expected {key} to validate");
        }
    }

    #[test]
    fn validate_key_rejects_empty() {
        assert!(validate_key("").is_err());
    }

    #[test]
    fn validate_key_rejects_absolute_and_traversal() {
        for key in ["/etc/passwd", "../secret", "foo/../bar", "ok/../../nope"] {
            assert!(validate_key(key).is_err(), "expected {key} to be rejected");
        }
    }

    #[test]
    fn validate_key_rejects_null_byte() {
        assert!(validate_key("chart\0.json").is_err());
    }
}
