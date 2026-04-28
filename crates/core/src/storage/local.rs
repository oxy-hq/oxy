use std::path::{Path, PathBuf};

use async_trait::async_trait;
use oxy_shared::errors::OxyError;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
};

use super::{BlobStorage, validate_key};

/// Filesystem-backed blob storage rooted at a fixed directory.
///
/// All keys resolve relative to `base_dir`. The base directory is created on
/// demand (on first `put`) so existing callers that previously pre-created
/// their charts/results directory keep working.
#[derive(Debug, Clone)]
pub struct LocalBlobStorage {
    base_dir: PathBuf,
}

impl LocalBlobStorage {
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }

    fn resolve(&self, key: &str) -> Result<PathBuf, OxyError> {
        validate_key(key)?;
        Ok(self.base_dir.join(key))
    }
}

#[async_trait]
impl BlobStorage for LocalBlobStorage {
    async fn put(&self, key: &str, data: Vec<u8>, _content_type: &str) -> Result<(), OxyError> {
        let path = self.resolve(key)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                OxyError::RuntimeError(format!(
                    "Failed to create blob storage directory {}: {e}",
                    parent.display()
                ))
            })?;
        }
        let mut file = fs::File::create(&path).await.map_err(|e| {
            OxyError::RuntimeError(format!(
                "Failed to create blob file {}: {e}",
                path.display()
            ))
        })?;
        file.write_all(&data).await.map_err(|e| {
            OxyError::RuntimeError(format!("Failed to write blob {}: {e}", path.display()))
        })?;
        // Tokio's AsyncWrite does not guarantee a flush on drop; do it explicitly
        // so callers reading back through `get` see the full payload without a
        // racing filesystem sync.
        file.flush().await.map_err(|e| {
            OxyError::RuntimeError(format!("Failed to flush blob {}: {e}", path.display()))
        })?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, OxyError> {
        let path = self.resolve(key)?;
        let mut file = fs::File::open(&path).await.map_err(|e| {
            OxyError::RuntimeError(format!("Failed to open blob {}: {e}", path.display()))
        })?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).await.map_err(|e| {
            OxyError::RuntimeError(format!("Failed to read blob {}: {e}", path.display()))
        })?;
        Ok(buf)
    }

    async fn public_url(&self, key: &str) -> Result<Option<String>, OxyError> {
        // Parity with S3BlobStorage::public_url — reject bad keys even though
        // local disk has no public URL to return, so a future implementation
        // swap can't introduce a silent validation regression.
        validate_key(key)?;
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn round_trips_bytes_through_disk() {
        let dir = TempDir::new().unwrap();
        let store = LocalBlobStorage::new(dir.path());
        store
            .put("chart.json", b"{\"ok\":true}".to_vec(), "application/json")
            .await
            .unwrap();
        let got = store.get("chart.json").await.unwrap();
        assert_eq!(got, b"{\"ok\":true}");
    }

    #[tokio::test]
    async fn creates_nested_directories() {
        let dir = TempDir::new().unwrap();
        let store = LocalBlobStorage::new(dir.path());
        store
            .put(
                "sub/dir/chart.json",
                vec![1, 2, 3],
                "application/octet-stream",
            )
            .await
            .unwrap();
        assert_eq!(
            store.get("sub/dir/chart.json").await.unwrap(),
            vec![1, 2, 3]
        );
    }

    #[tokio::test]
    async fn rejects_keys_escaping_base_dir() {
        let dir = TempDir::new().unwrap();
        let store = LocalBlobStorage::new(dir.path());
        assert!(store.put("../escape.json", vec![0], "x").await.is_err());
        assert!(store.get("../escape.json").await.is_err());
    }

    #[tokio::test]
    async fn public_url_is_none() {
        let dir = TempDir::new().unwrap();
        let store = LocalBlobStorage::new(dir.path());
        assert_eq!(store.public_url("anything").await.unwrap(), None);
    }
}
