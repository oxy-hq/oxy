use std::time::Duration;

use async_trait::async_trait;
use aws_sdk_s3::{Client, presigning::PresigningConfig, primitives::ByteStream};
use oxy_shared::errors::OxyError;

use super::{BlobStorage, validate_key};

/// Runtime configuration for an S3 blob storage backend.
///
/// Credentials are intentionally NOT part of this struct — the AWS SDK picks
/// them up via its standard credential chain (env vars, shared config, IAM
/// role, etc.). This matches how other AWS-using code in the workspace
/// (email/SES) authenticates and keeps secret handling out of Oxy.
#[derive(Debug, Clone)]
pub struct S3BlobStorageConfig {
    pub bucket: String,
    pub region: Option<String>,
    /// Optional key prefix. If set, every blob key is stored as
    /// `{prefix}/{key}`. Leading/trailing slashes are normalized away.
    pub prefix: Option<String>,
    /// Optional base URL for an externally-served public path, e.g.
    /// `https://cdn.example.com`. When set, `public_url()` returns
    /// `{base}/{object_key}` verbatim — used when a CDN (CloudFront,
    /// Cloudflare, etc.) sits in front of the bucket and the operator
    /// owns auth there. **Incompatible with private buckets.**
    ///
    /// When unset (the default), `public_url()` returns a presigned GET
    /// URL valid for `presign_ttl`. This is the only mode that works for
    /// SSE-KMS and "all public access blocked" buckets.
    pub public_url_base: Option<String>,
    /// Time-to-live for presigned GET URLs returned by `public_url()`
    /// when no `public_url_base` is configured.
    ///
    /// Note: presigned URLs signed by short-lived credentials (EKS Pod
    /// Identity / IRSA) are valid for `min(ttl, ~remaining_credential_lifetime)`.
    /// For Slack-style use cases (CDN fetches and caches within seconds),
    /// any reasonable TTL is fine.
    pub presign_ttl: Duration,
}

#[derive(Debug, Clone)]
pub struct S3BlobStorage {
    client: Client,
    bucket: String,
    prefix: Option<String>,
    public_url_base: Option<String>,
    presign_ttl: Duration,
}

impl S3BlobStorage {
    pub async fn new(config: S3BlobStorageConfig) -> Result<Self, OxyError> {
        let mut loader = aws_config::from_env();
        if let Some(region) = &config.region {
            loader = loader.region(aws_config::Region::new(region.clone()));
        }
        let sdk_config = loader.load().await;
        let client = Client::new(&sdk_config);

        let prefix = normalize_prefix(config.prefix.as_deref());

        Ok(Self {
            client,
            bucket: config.bucket,
            prefix,
            public_url_base: config.public_url_base,
            presign_ttl: config.presign_ttl,
        })
    }

    fn object_key(&self, key: &str) -> String {
        join_with_prefix(self.prefix.as_deref(), key)
    }
}

/// Trim leading/trailing slashes off the prefix and drop it entirely if
/// that leaves nothing. Keeps S3 object keys free of accidental `//` and
/// empty-segment foot-guns.
fn normalize_prefix(raw: Option<&str>) -> Option<String> {
    raw.map(|p| p.trim_matches('/').to_string())
        .filter(|p| !p.is_empty())
}

/// Compose a final S3 object key from a normalized prefix and a user key.
fn join_with_prefix(prefix: Option<&str>, key: &str) -> String {
    match prefix {
        Some(prefix) => format!("{prefix}/{key}"),
        None => key.to_string(),
    }
}

#[async_trait]
impl BlobStorage for S3BlobStorage {
    async fn put(&self, key: &str, data: Vec<u8>, content_type: &str) -> Result<(), OxyError> {
        validate_key(key)?;
        let object_key = self.object_key(key);
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .content_type(content_type)
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!(
                    "Failed to upload {} to s3://{}/{}: {e}",
                    key, self.bucket, object_key
                ))
            })?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, OxyError> {
        validate_key(key)?;
        let object_key = self.object_key(key);
        let output = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .send()
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!(
                    "Failed to fetch s3://{}/{}: {e}",
                    self.bucket, object_key
                ))
            })?;
        let bytes = output.body.collect().await.map_err(|e| {
            OxyError::RuntimeError(format!(
                "Failed to read body of s3://{}/{}: {e}",
                self.bucket, object_key
            ))
        })?;
        Ok(bytes.into_bytes().to_vec())
    }

    /// Return a URL the caller (Slack's CDN, a browser, etc.) can fetch.
    ///
    /// Two modes, picked at config time:
    ///
    /// 1. **`public_url_base` set** — returns `{base}/{object_key}` verbatim.
    ///    This is the CDN/CloudFront path; the operator owns auth.
    ///
    /// 2. **`public_url_base` unset** (default) — returns a presigned GET URL
    ///    valid for `presign_ttl`. This is the only mode that works for
    ///    private buckets, SSE-KMS, and "all public access blocked".
    ///    No extra IAM is required; the role that uploads also signs GETs.
    ///
    /// **Credential-expiry caveat**: when running under EKS Pod Identity /
    /// IRSA / similar short-lived credential providers, the presigned URL
    /// is valid for `min(presign_ttl, remaining_credential_lifetime)`.
    /// In Slack-style flows where the CDN fetches once within seconds and
    /// caches the bytes, this is a non-issue — but worth knowing if a
    /// downstream caller stashes the URL for hours.
    async fn public_url(&self, key: &str) -> Result<Option<String>, OxyError> {
        validate_key(key)?;
        let object_key = self.object_key(key);

        if let Some(base) = &self.public_url_base {
            return Ok(Some(format!(
                "{}/{}",
                base.trim_end_matches('/'),
                object_key
            )));
        }

        let presigning = PresigningConfig::expires_in(self.presign_ttl).map_err(|e| {
            OxyError::RuntimeError(format!("invalid presign TTL {:?}: {e}", self.presign_ttl))
        })?;

        let presigned = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .presigned(presigning)
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!(
                    "failed to presign s3://{}/{}: {e}",
                    self.bucket, object_key
                ))
            })?;

        Ok(Some(presigned.uri().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_prefix_strips_slashes_and_collapses_empty() {
        assert_eq!(normalize_prefix(None), None);
        assert_eq!(normalize_prefix(Some("")), None);
        assert_eq!(normalize_prefix(Some("/")), None);
        assert_eq!(normalize_prefix(Some("///")), None);
        assert_eq!(normalize_prefix(Some("charts")), Some("charts".into()));
        assert_eq!(normalize_prefix(Some("/charts/")), Some("charts".into()));
        assert_eq!(
            normalize_prefix(Some("/nested/path/")),
            Some("nested/path".into())
        );
    }

    #[test]
    fn join_with_prefix_inserts_single_slash_separator() {
        assert_eq!(join_with_prefix(None, "x.png"), "x.png");
        assert_eq!(join_with_prefix(Some("charts"), "x.png"), "charts/x.png");
        assert_eq!(join_with_prefix(Some("a/b"), "sub/x.png"), "a/b/sub/x.png");
    }

    /// Build an `S3BlobStorage` with stub static credentials so presigning
    /// runs entirely offline (no network, no IMDS). The presign signature
    /// is computed locally — only `put`/`get` need real AWS auth.
    fn stub_storage(public_url_base: Option<&str>, prefix: Option<&str>) -> S3BlobStorage {
        use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
        let conf = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new("us-east-1"))
            .credentials_provider(Credentials::new(
                "AKIATESTACCESSKEY",
                "test_secret_access_key",
                None,
                None,
                "test",
            ))
            .build();
        S3BlobStorage {
            client: Client::from_conf(conf),
            bucket: "test-bucket".to_string(),
            prefix: prefix.map(str::to_string),
            public_url_base: public_url_base.map(str::to_string),
            presign_ttl: Duration::from_secs(3600),
        }
    }

    #[tokio::test]
    async fn public_url_returns_presigned_url_by_default() {
        let storage = stub_storage(None, None);
        let url = storage
            .public_url("chart.png")
            .await
            .expect("public_url ok")
            .expect("url present");

        // Virtual-hosted style with the bucket and region.
        assert!(
            url.starts_with("https://test-bucket.s3.us-east-1.amazonaws.com/chart.png?"),
            "expected virtual-hosted style with query string, got: {url}"
        );
        // SigV4 marker — proves we actually presigned and didn't fall back
        // to a plain URL.
        assert!(
            url.contains("X-Amz-Signature="),
            "expected SigV4 signature in presigned URL, got: {url}"
        );
        assert!(
            url.contains("X-Amz-Expires="),
            "expected expiry in presigned URL, got: {url}"
        );
    }

    #[tokio::test]
    async fn public_url_honours_prefix_in_presigned_path() {
        let storage = stub_storage(None, Some("charts"));
        let url = storage
            .public_url("foo.png")
            .await
            .expect("public_url ok")
            .expect("url present");

        assert!(
            url.starts_with("https://test-bucket.s3.us-east-1.amazonaws.com/charts/foo.png?"),
            "prefix should appear in object key path, got: {url}"
        );
    }

    #[tokio::test]
    async fn public_url_uses_cdn_base_when_set_and_skips_presigning() {
        let storage = stub_storage(Some("https://cdn.example.com"), None);
        let url = storage
            .public_url("chart.png")
            .await
            .expect("public_url ok")
            .expect("url present");

        // Exactly `{base}/{key}` — no presigning, no query string.
        assert_eq!(url, "https://cdn.example.com/chart.png");
    }

    #[tokio::test]
    async fn public_url_trims_trailing_slash_on_cdn_base() {
        let storage = stub_storage(Some("https://cdn.example.com/"), None);
        let url = storage
            .public_url("chart.png")
            .await
            .expect("public_url ok")
            .expect("url present");

        assert_eq!(url, "https://cdn.example.com/chart.png");
    }
}
