use async_trait::async_trait;
use aws_sdk_s3::{Client, primitives::ByteStream, types::ObjectCannedAcl};
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
    /// Optional base URL to use when constructing a public URL, e.g.
    /// `https://cdn.example.com`. When unset, defaults to the virtual-hosted
    /// S3 URL: `https://{bucket}.s3.{region}.amazonaws.com`.
    pub public_url_base: Option<String>,
    /// Optional canned ACL applied to every uploaded object. Most modern
    /// buckets disable ACLs and rely on bucket policies instead, so this is
    /// `None` by default. Set to `"public-read"` when the bucket allows it.
    pub acl: Option<String>,
}

#[derive(Debug, Clone)]
pub struct S3BlobStorage {
    client: Client,
    bucket: String,
    region: String,
    prefix: Option<String>,
    public_url_base: Option<String>,
    acl: Option<ObjectCannedAcl>,
}

impl S3BlobStorage {
    pub async fn new(config: S3BlobStorageConfig) -> Result<Self, OxyError> {
        let mut loader = aws_config::from_env();
        if let Some(region) = &config.region {
            loader = loader.region(aws_config::Region::new(region.clone()));
        }
        let sdk_config = loader.load().await;
        let client = Client::new(&sdk_config);

        let region = config
            .region
            .clone()
            .or_else(|| sdk_config.region().map(|r| r.as_ref().to_string()))
            .unwrap_or_else(|| "us-east-1".to_string());

        let prefix = normalize_prefix(config.prefix.as_deref());
        let acl = config.acl.as_deref().map(ObjectCannedAcl::from);

        Ok(Self {
            client,
            bucket: config.bucket,
            region,
            prefix,
            public_url_base: config.public_url_base,
            acl,
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

/// Build the publicly reachable URL for an already-prefixed object key.
/// When a `public_url_base` is configured we honour it (trimming a trailing
/// slash so the caller can write the base either way). Otherwise we fall
/// back to the virtual-hosted S3 URL derived from bucket + region.
fn build_public_url(
    public_url_base: Option<&str>,
    bucket: &str,
    region: &str,
    object_key: &str,
) -> String {
    match public_url_base {
        Some(base) => format!("{}/{}", base.trim_end_matches('/'), object_key),
        None => format!("https://{bucket}.s3.{region}.amazonaws.com/{object_key}"),
    }
}

#[async_trait]
impl BlobStorage for S3BlobStorage {
    async fn put(&self, key: &str, data: Vec<u8>, content_type: &str) -> Result<(), OxyError> {
        validate_key(key)?;
        let object_key = self.object_key(key);
        let mut request = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .content_type(content_type)
            .body(ByteStream::from(data));
        if let Some(acl) = self.acl.as_ref() {
            request = request.acl(acl.clone());
        }
        request.send().await.map_err(|e| {
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

    async fn public_url(&self, key: &str) -> Result<Option<String>, OxyError> {
        validate_key(key)?;
        let object_key = self.object_key(key);
        Ok(Some(build_public_url(
            self.public_url_base.as_deref(),
            &self.bucket,
            &self.region,
            &object_key,
        )))
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

    #[test]
    fn build_public_url_uses_virtual_hosted_style_by_default() {
        let url = build_public_url(None, "my-bucket", "us-east-1", "charts/x.png");
        assert_eq!(
            url,
            "https://my-bucket.s3.us-east-1.amazonaws.com/charts/x.png"
        );
    }

    #[test]
    fn build_public_url_honours_configured_base_and_trims_trailing_slash() {
        let with_slash =
            build_public_url(Some("https://cdn.example.com/"), "b", "r", "charts/x.png");
        let without_slash =
            build_public_url(Some("https://cdn.example.com"), "b", "r", "charts/x.png");
        assert_eq!(with_slash, "https://cdn.example.com/charts/x.png");
        assert_eq!(without_slash, "https://cdn.example.com/charts/x.png");
    }

    #[test]
    fn acl_round_trips_through_object_canned_acl() {
        // Guard against a future aws-sdk-s3 update that silently routes a
        // typo'd ACL string into `Unknown(_)` — catching it here is much
        // cheaper than debugging a 400 from S3 in production.
        let acl = ObjectCannedAcl::from("public-read");
        assert_eq!(acl.as_str(), "public-read");
    }
}
