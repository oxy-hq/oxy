//! Chart image publishing.
//!
//! The web app renders chart JSON client-side via echarts. That flow continues
//! to be the source of truth — the JSON lives on disk and the frontend fetches
//! it through `/charts/{file}`. For integrations that cannot run a browser
//! (Slack unfurls, external embeds, docs exports), we additionally mirror the
//! same chart as a PNG image at a publicly reachable URL.
//!
//! The publishing pipeline has two pluggable pieces:
//!
//! * [`ChartImageRenderer`] — turns a chart JSON config into PNG bytes.
//! * [`crate::storage::BlobStorage`] — persists those bytes and (optionally)
//!   returns a public URL.
//!
//! [`BlobStorageChartImagePublisher`] wires them together. Core only defines
//! the trait for the renderer; the concrete implementation is supplied by a
//! higher layer (the CLI crate already drives `headless_chrome` for PR #1668
//! server-side chart export — the Slack integration should reuse that rather
//! than build a parallel pipeline).

use async_trait::async_trait;
use oxy_shared::errors::OxyError;
use serde_json::Value;
use std::sync::Arc;

use super::BlobStorage;

/// Converts an echarts JSON config into PNG bytes.
///
/// Implementations are expected to be expensive (they typically drive a
/// headless browser). Errors propagate through the publisher to the caller;
/// the caller decides whether to surface them, retry, or log-and-continue
/// (e.g. the Slack unfurl path may choose best-effort, while a synchronous
/// "render this chart" API would bubble the error up).
#[async_trait]
pub trait ChartImageRenderer: Send + Sync + std::fmt::Debug {
    async fn render_png(&self, config: &Value) -> Result<Vec<u8>, OxyError>;
}

pub type SharedChartImageRenderer = Arc<dyn ChartImageRenderer>;

/// Publishes a rendered chart image to a backing blob store and returns a
/// public URL for external consumers (Slack, embeds, etc.).
#[async_trait]
pub trait ChartImagePublisher: Send + Sync + std::fmt::Debug {
    /// Render `config` to PNG and upload under `key`. Returns the public URL
    /// for the uploaded image when the backing store exposes one.
    async fn publish(&self, key: &str, config: &Value) -> Result<Option<String>, OxyError>;
}

pub type SharedChartImagePublisher = Arc<dyn ChartImagePublisher>;

/// Default publisher: renders PNG via the injected [`ChartImageRenderer`],
/// uploads through any [`BlobStorage`], and asks that store for a public URL.
///
/// Name reflects the actual behavior — it works with local disk or S3 or any
/// other future backend that implements [`BlobStorage`].
#[derive(Debug, Clone)]
pub struct BlobStorageChartImagePublisher {
    renderer: SharedChartImageRenderer,
    blob_storage: Arc<dyn BlobStorage>,
}

impl BlobStorageChartImagePublisher {
    pub fn new(renderer: SharedChartImageRenderer, blob_storage: Arc<dyn BlobStorage>) -> Self {
        Self {
            renderer,
            blob_storage,
        }
    }
}

#[async_trait]
impl ChartImagePublisher for BlobStorageChartImagePublisher {
    async fn publish(&self, key: &str, config: &Value) -> Result<Option<String>, OxyError> {
        // Fail fast on a bad key so we never pay the headless-browser render
        // cost just to have the underlying blob store reject it on `put`.
        super::validate_key(key)?;
        let png = self.renderer.render_png(config).await?;
        self.blob_storage.put(key, png, "image/png").await?;
        self.blob_storage.public_url(key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::LocalBlobStorage;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    #[derive(Debug)]
    struct StubRenderer {
        png: Vec<u8>,
        calls: AtomicUsize,
    }

    #[async_trait]
    impl ChartImageRenderer for StubRenderer {
        async fn render_png(&self, _config: &Value) -> Result<Vec<u8>, OxyError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.png.clone())
        }
    }

    #[tokio::test]
    async fn publishes_rendered_bytes_and_returns_backend_public_url() {
        let dir = TempDir::new().unwrap();
        let renderer = Arc::new(StubRenderer {
            png: vec![0x89, b'P', b'N', b'G'],
            calls: AtomicUsize::new(0),
        });
        let blob_storage: Arc<dyn BlobStorage> = Arc::new(LocalBlobStorage::new(dir.path()));
        let publisher = BlobStorageChartImagePublisher::new(renderer.clone(), blob_storage.clone());

        let url = publisher
            .publish("chart.png", &json!({ "title": "x" }))
            .await
            .unwrap();

        assert_eq!(renderer.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            blob_storage.get("chart.png").await.unwrap(),
            vec![0x89, b'P', b'N', b'G']
        );
        // LocalBlobStorage has no notion of public URL; the pipeline must pass
        // that through faithfully rather than fabricating one.
        assert_eq!(url, None);
    }

    #[tokio::test]
    async fn propagates_renderer_error_without_writing_to_storage() {
        #[derive(Debug)]
        struct FailingRenderer;
        #[async_trait]
        impl ChartImageRenderer for FailingRenderer {
            async fn render_png(&self, _: &Value) -> Result<Vec<u8>, OxyError> {
                Err(OxyError::RuntimeError("render blew up".into()))
            }
        }

        let dir = TempDir::new().unwrap();
        let blob_storage: Arc<dyn BlobStorage> = Arc::new(LocalBlobStorage::new(dir.path()));
        let publisher =
            BlobStorageChartImagePublisher::new(Arc::new(FailingRenderer), blob_storage.clone());

        let err = publisher
            .publish("chart.png", &json!({}))
            .await
            .expect_err("renderer failure must propagate");
        assert!(err.to_string().contains("render blew up"));
        // Nothing should have been written to the backing store.
        assert!(blob_storage.get("chart.png").await.is_err());
    }
}
