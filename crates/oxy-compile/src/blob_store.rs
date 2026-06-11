//! S3-backed storage for the compile boundary's semantic blobs.
//!
//! Semantic views and topics can routinely top tens of KB each across
//! the five warehouse dialects we compile against. At 100K active
//! workspaces × N views per workspace × revisions per workspace, the
//! `semantic_views.definition` / `semantic_topics.definition` JSONB
//! columns become the dominant Postgres tablespace cost. This module
//! moves the canonical body to S3 keyed by content hash, keeping
//! Postgres as the addressable identity (`(revision, name) → key`).
//!
//! Gating: `OXY_COMPILE_BLOB_S3_BUCKET`. When unset, every public fn
//! returns `Ok(None)` — the writer stores `compiled_sql_blob_key =
//! NULL` and downstream readers fall back to the in-row `definition`
//! JSONB. When set, the writer uploads each view/topic body and stores
//! the key; reader-side wiring fetches from S3 first, falls back to
//! Postgres on miss / 404 / transport error so a Postgres-only
//! deployment continues working.
//!
//! Pod identity is the auth model — `aws_config::load_defaults` picks
//! up the STS session attached to the workload's service account.
//! Same shape as `customer_apps_build_store` in oxy-app.
//!
//! Keys are content-addressed (`sha256(body)[..32]`) so identical
//! blobs across workspaces share storage; the `(revision_id, name) →
//! key` mapping in Postgres handles per-revision identity. The
//! workspace prefix is preserved so an operator can scope a per-
//! workspace lifecycle / DELETE.

use aws_config::BehaviorVersion;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::primitives::ByteStream;
use sha2::{Digest, Sha256};
use tokio::sync::OnceCell;
use uuid::Uuid;

/// Process-wide cached S3 client. `aws_config::load_defaults` walks the
/// credential chain (IMDS, env vars, shared config, pod identity) on
/// every call — ~tens of ms. The compile worker uploads N+M blobs per
/// revision and semantic queries fetch on every materialise; caching
/// the client makes those constant overhead. Safe under concurrent
/// initialisation: `OnceCell` serialises racing calls.
static S3_CLIENT: OnceCell<S3Client> = OnceCell::const_new();

const ENV_BUCKET: &str = "OXY_COMPILE_BLOB_S3_BUCKET";

/// Returns the configured bucket name, trimmed; `None` when unset or
/// empty. Cheap env-var read; callers may call it once per row.
pub fn bucket() -> Option<String> {
    std::env::var(ENV_BUCKET)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Which compiled artifact lives at the key. Scopes the prefix so an
/// S3 listing reads like the on-disk source tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlobKind {
    SemanticView,
    SemanticTopic,
}

impl BlobKind {
    fn prefix(self) -> &'static str {
        match self {
            BlobKind::SemanticView => "semantic_views",
            BlobKind::SemanticTopic => "semantic_topics",
        }
    }
}

/// Canonical S3 key for a body. Content-addressed so two identical
/// blobs across workspaces share storage; workspace prefix retained
/// for per-workspace lifecycle policies.
pub fn canonical_key(workspace_id: Uuid, kind: BlobKind, name: &str, body: &[u8]) -> String {
    let sha = hex::encode(Sha256::digest(body));
    let short_sha = &sha[..32];
    format!(
        "workspaces/{workspace_id}/{prefix}/{name}-{short_sha}.yml",
        prefix = kind.prefix()
    )
}

/// Upload a body to S3 under the canonical key. Returns the key on
/// success; `Ok(None)` when no bucket is configured (caller stores
/// NULL key); `Err` on transport / permission failure.
pub async fn put_blob(
    workspace_id: Uuid,
    kind: BlobKind,
    name: &str,
    body: &[u8],
) -> Result<Option<String>, BlobError> {
    let Some(bucket) = bucket() else {
        return Ok(None);
    };
    let key = canonical_key(workspace_id, kind, name, body);
    let client = s3_client().await;
    client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from(body.to_vec()))
        .content_type("application/yaml")
        .send()
        .await
        .map_err(|e| BlobError::Transport(format!("{e}")))?;
    tracing::debug!(
        workspace_id = %workspace_id,
        key,
        kind = ?kind,
        bytes = body.len(),
        "compiled blob: uploaded to S3"
    );
    Ok(Some(key))
}

/// Fetch a body by key. `Ok(None)` when no bucket is configured (the
/// caller should fall back to the in-row `definition`); `Err` on
/// 404 / transport / permission failure.
pub async fn get_blob(key: &str) -> Result<Option<Vec<u8>>, BlobError> {
    let Some(bucket) = bucket() else {
        return Ok(None);
    };
    let client = s3_client().await;
    let resp = client
        .get_object()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| BlobError::Transport(format!("{e}")))?;
    let bytes = resp
        .body
        .collect()
        .await
        .map_err(|e| BlobError::Transport(format!("read body: {e}")))?
        .into_bytes();
    Ok(Some(bytes.to_vec()))
}

#[derive(Debug)]
pub enum BlobError {
    Transport(String),
}

impl std::fmt::Display for BlobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlobError::Transport(msg) => write!(f, "compiled blob transport error: {msg}"),
        }
    }
}

impl std::error::Error for BlobError {}

async fn s3_client() -> &'static S3Client {
    S3_CLIENT
        .get_or_init(|| async {
            let shared = aws_config::load_defaults(BehaviorVersion::latest()).await;
            let mut builder = aws_sdk_s3::config::Builder::from(&shared);
            if std::env::var("AWS_ENDPOINT_URL")
                .ok()
                .is_some_and(|v| !v.trim().is_empty())
            {
                builder = builder.force_path_style(true);
            }
            S3Client::from_conf(builder.build())
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_key_is_content_addressed_with_workspace_prefix() {
        let ws = Uuid::nil();
        let a = canonical_key(ws, BlobKind::SemanticView, "orders", b"select 1");
        let b = canonical_key(ws, BlobKind::SemanticView, "orders", b"select 1");
        assert_eq!(a, b, "same workspace + name + body must yield same key");
        let different_body = canonical_key(ws, BlobKind::SemanticView, "orders", b"select 2");
        assert_ne!(a, different_body, "different body must yield different key");
        assert!(a.starts_with(&format!("workspaces/{ws}/semantic_views/orders-")));
        assert!(a.ends_with(".yml"));
    }

    #[test]
    fn bucket_kind_prefixes() {
        assert_eq!(BlobKind::SemanticView.prefix(), "semantic_views");
        assert_eq!(BlobKind::SemanticTopic.prefix(), "semantic_topics");
    }
}
