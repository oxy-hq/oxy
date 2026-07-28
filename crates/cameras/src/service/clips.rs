//! Presigned-URL clip archival (Tier B #6 follow-up).
//!
//! **Why presigned URLs and not worker-held creds.** Edge boxes
//! sit in restaurant back rooms with uncontrolled physical
//! access; staff turnover means we can't treat the box as a
//! trusted credential vault. The V1 of clip archival had the
//! worker hold AWS keys and PUT directly — convenient, but a
//! single compromised box leaks credentials with whatever IAM
//! scope they carry, and even narrowly-scoped policies leak
//! cross-tenant inside the bucket if mis-policied. This module
//! moves the AWS principal entirely server-side:
//!
//!   1. Worker has a violation → POSTs `/control/clips/sign`
//!      with `{report_id, content_length}`.
//!   2. Server returns a presigned PUT URL valid for ~5 minutes
//!      against `s3://{bucket}/{prefix}/{workspace_id}/{date}/
//!      {report_id}.mp4`.
//!   3. Worker PUTs the bytes to that URL.
//!   4. Worker includes `evidence_s3_key` in the next compliance
//!      batch.
//!
//! Even if a box is fully compromised, the attacker can at most
//! upload bytes to their own future report ids — the URL doesn't
//! grant Get, List, Delete, or access to anything outside the
//! workspace prefix.
//!
//! ## Configuration
//!
//! These vars live under the camera-specific `OXY_CAMERAS_CLIPS_S3_*`
//! namespace (both compliance and congestion evidence clips share this
//! one bucket). The generic `OXY_S3_*` namespace is deliberately left
//! free for any future workspace-wide S3 use so the two never collide.
//!
//! The server reads:
//!
//!   - `OXY_CAMERAS_CLIPS_S3_BUCKET` — required; absence disables both the
//!     mint and the playback endpoint (501).
//!   - `OXY_CAMERAS_CLIPS_S3_REGION` — defaults to `us-east-1`.
//!   - `OXY_CAMERAS_CLIPS_S3_KEY_PREFIX` — defaults to `compliance`.
//!   - `OXY_CAMERAS_CLIPS_S3_PRESIGN_TTL_SECONDS` — defaults to 300 (5 min).
//!     Workers can tolerate a couple of seconds of skew; bigger
//!     values just widen the attacker's replay window for a URL
//!     that's already bounded by `Content-Length`.
//!
//! AWS credentials come from the standard SDK chain: env vars,
//! IAM role, instance profile, etc. The cameras crate never
//! reads `AWS_ACCESS_KEY_ID` directly.

use crate::service::{ServiceError, ServiceResult};
use aws_sdk_s3::Client;
use aws_sdk_s3::presigning::PresigningConfig;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::OnceCell;
use uuid::Uuid;

/// What the worker hands us when asking for a PUT URL.
pub struct MintPutInput {
    pub workspace_id: Uuid,
    pub report_id: Uuid,
    pub segment_start: DateTime<Utc>,
    /// Used to add a `Content-Length` constraint to the presign,
    /// so a leaked URL can't be reused to push an arbitrarily-
    /// large blob.
    pub content_length: i64,
}

/// What the worker gets back from `/control/clips/sign`.
#[derive(Debug, Clone, Serialize)]
pub struct MintPutOutput {
    pub presigned_put_url: String,
    pub key: String,
    pub expires_in_secs: u64,
}

/// What the operator UI gets back from
/// `/{wid}/cameras/clips/{report_id}/url`.
#[derive(Debug, Clone, Serialize)]
pub struct MintGetOutput {
    pub presigned_get_url: String,
    pub expires_in_secs: u64,
}

#[derive(Debug, Clone)]
pub struct S3Config {
    pub bucket: String,
    pub region: String,
    pub key_prefix: String,
    pub presign_ttl: Duration,
}

impl S3Config {
    /// Resolve from env. `None` when `OXY_CAMERAS_CLIPS_S3_BUCKET` is unset —
    /// the deployment hasn't opted into clip archival.
    pub fn from_env() -> Option<Self> {
        let bucket = std::env::var("OXY_CAMERAS_CLIPS_S3_BUCKET")
            .ok()?
            .trim()
            .to_string();
        if bucket.is_empty() {
            return None;
        }
        let region = std::env::var("OXY_CAMERAS_CLIPS_S3_REGION")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "us-east-1".to_string());
        let key_prefix = std::env::var("OXY_CAMERAS_CLIPS_S3_KEY_PREFIX")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "compliance".to_string());
        let presign_ttl = std::env::var("OXY_CAMERAS_CLIPS_S3_PRESIGN_TTL_SECONDS")
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(300));
        Some(Self {
            bucket,
            region,
            key_prefix,
            presign_ttl,
        })
    }
}

/// Cached `(S3Config, Client)` pair. Building the SDK client
/// once and reusing it across requests is the documented
/// recommendation — it caches credentials provider chain output
/// internally, and re-resolving on every request would re-hit
/// IMDS or the credential file each time.
static CONFIG_CACHE: OnceLock<Option<S3Config>> = OnceLock::new();
static CLIENT_CACHE: OnceCell<Client> = OnceCell::const_new();

/// Resolve `S3Config` once per process. Re-checks env on first
/// call only; subsequent calls hit the cache. If you need to
/// pick up an env change you have to restart the process —
/// matches every other "config from env at boot" surface in the
/// codebase.
pub fn config() -> Option<&'static S3Config> {
    CONFIG_CACHE.get_or_init(S3Config::from_env).as_ref()
}

async fn client() -> ServiceResult<&'static Client> {
    CLIENT_CACHE
        .get_or_init(|| async {
            let region = config()
                .map(|c| c.region.clone())
                .unwrap_or_else(|| "us-east-1".to_string());
            let shared = aws_config::defaults(aws_config::BehaviorVersion::latest())
                .region(aws_config::Region::new(region))
                .load()
                .await;
            Client::new(&shared)
        })
        .await;
    // Per OnceCell docs, after get_or_init we know it's set.
    Ok(CLIENT_CACHE.get().expect("client just initialized"))
}

/// Build the bucket-relative key for a violation clip. Same
/// shape the worker used to compute itself; centralizing here
/// means there's exactly one source of truth and the worker
/// can't accidentally diverge.
pub fn build_key(cfg: &S3Config, workspace_id: Uuid, report_id: Uuid, ts: DateTime<Utc>) -> String {
    let date = ts.format("%Y-%m-%d");
    format!(
        "{}/{}/{}/{}.mp4",
        cfg.key_prefix, workspace_id, date, report_id
    )
}

/// Mint a presigned PUT URL for the worker to upload a violation
/// clip. The URL is bound to:
///
///   - exact bucket + key
///   - exact `Content-Length`
///   - `Content-Type: video/mp4`
///   - TTL from [`S3Config::presign_ttl`]
///
/// All four constraints baked in: a leaked URL can't be reused
/// for a different size, a different content type, or beyond
/// the window.
pub async fn mint_put_url(input: MintPutInput) -> ServiceResult<MintPutOutput> {
    let cfg = config().ok_or(ServiceError::Unavailable(
        "OXY_CAMERAS_CLIPS_S3_BUCKET is not configured; clip archival disabled",
    ))?;
    if input.content_length <= 0 {
        return Err(ServiceError::InvalidInput(
            "content_length must be > 0".into(),
        ));
    }
    let key = build_key(
        cfg,
        input.workspace_id,
        input.report_id,
        input.segment_start,
    );
    let presigning = PresigningConfig::expires_in(cfg.presign_ttl).map_err(|e| {
        ServiceError::Unavailable(Box::leak(
            format!("invalid presign config: {e}").into_boxed_str(),
        ))
    })?;
    let c = client().await?;
    let req = c
        .put_object()
        .bucket(&cfg.bucket)
        .key(&key)
        .content_length(input.content_length)
        .content_type("video/mp4")
        .presigned(presigning)
        .await
        .map_err(|e| {
            tracing::warn!(
                bucket = %cfg.bucket,
                key,
                error = %e,
                "clips.mint_put_url: presign failed"
            );
            ServiceError::Upstream(format!("s3 presign put: {e}"))
        })?;
    Ok(MintPutOutput {
        presigned_put_url: req.uri().to_string(),
        key,
        expires_in_secs: cfg.presign_ttl.as_secs(),
    })
}

/// Mint a presigned GET URL for operator-facing playback. The
/// `key` is the one the worker stamped on the compliance report;
/// we re-validate that it lives under this workspace's prefix
/// so an operator can't read another tenant's clips by handing
/// us a foreign key.
pub async fn mint_get_url(workspace_id: Uuid, key: &str) -> ServiceResult<MintGetOutput> {
    let cfg = config().ok_or(ServiceError::Unavailable(
        "OXY_CAMERAS_CLIPS_S3_BUCKET is not configured; clip playback disabled",
    ))?;
    let expected_prefix = format!("{}/{}/", cfg.key_prefix, workspace_id);
    if !key.starts_with(&expected_prefix) {
        // Defense-in-depth: the route layer already enforces
        // ownership via the compliance-reports row, but a
        // belt-and-suspenders check here means we'd never sign
        // a cross-tenant key even if a future caller forgets the
        // outer check.
        return Err(ServiceError::Forbidden(
            "clip key does not belong to this workspace",
        ));
    }
    let presigning = PresigningConfig::expires_in(cfg.presign_ttl).map_err(|e| {
        ServiceError::Unavailable(Box::leak(
            format!("invalid presign config: {e}").into_boxed_str(),
        ))
    })?;
    let c = client().await?;
    let req = c
        .get_object()
        .bucket(&cfg.bucket)
        .key(key)
        .presigned(presigning)
        .await
        .map_err(|e| {
            tracing::warn!(
                bucket = %cfg.bucket,
                key,
                error = %e,
                "clips.mint_get_url: presign failed"
            );
            ServiceError::Upstream(format!("s3 presign get: {e}"))
        })?;
    Ok(MintGetOutput {
        presigned_get_url: req.uri().to_string(),
        expires_in_secs: cfg.presign_ttl.as_secs(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn build_key_partitions_by_workspace_and_date() {
        let cfg = S3Config {
            bucket: "b".into(),
            region: "us-east-1".into(),
            key_prefix: "compliance".into(),
            presign_ttl: Duration::from_secs(300),
        };
        let wid = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let rid = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();
        let ts = chrono::DateTime::parse_from_rfc3339("2026-05-30T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            build_key(&cfg, wid, rid, ts),
            "compliance/00000000-0000-0000-0000-000000000001/2026-05-30/\
             00000000-0000-0000-0000-000000000002.mp4"
        );
    }

    #[serial]
    #[test]
    fn from_env_returns_none_when_bucket_unset() {
        // SAFETY: tests run single-threaded under nextest's default.
        unsafe {
            std::env::remove_var("OXY_CAMERAS_CLIPS_S3_BUCKET");
        }
        assert!(S3Config::from_env().is_none());
    }

    #[serial]
    #[test]
    fn from_env_picks_up_overrides() {
        // SAFETY: see above.
        unsafe {
            std::env::set_var("OXY_CAMERAS_CLIPS_S3_BUCKET", "my-bucket");
            std::env::set_var("OXY_CAMERAS_CLIPS_S3_REGION", "eu-west-2");
            std::env::set_var("OXY_CAMERAS_CLIPS_S3_KEY_PREFIX", "incidents");
            std::env::set_var("OXY_CAMERAS_CLIPS_S3_PRESIGN_TTL_SECONDS", "60");
        }
        let cfg = S3Config::from_env().expect("bucket set");
        assert_eq!(cfg.bucket, "my-bucket");
        assert_eq!(cfg.region, "eu-west-2");
        assert_eq!(cfg.key_prefix, "incidents");
        assert_eq!(cfg.presign_ttl, Duration::from_secs(60));
        unsafe {
            std::env::remove_var("OXY_CAMERAS_CLIPS_S3_BUCKET");
            std::env::remove_var("OXY_CAMERAS_CLIPS_S3_REGION");
            std::env::remove_var("OXY_CAMERAS_CLIPS_S3_KEY_PREFIX");
            std::env::remove_var("OXY_CAMERAS_CLIPS_S3_PRESIGN_TTL_SECONDS");
        }
    }
}
