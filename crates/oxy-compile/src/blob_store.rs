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
//! Same shape as `custom_apps_build_store` in oxy-app.
//!
//! Keys are content-addressed (`sha256(body)[..32]`) so identical
//! blobs across workspaces share storage; the `(revision_id, name) →
//! key` mapping in Postgres handles per-revision identity. The
//! workspace prefix is preserved so an operator can scope a per-
//! workspace lifecycle / DELETE.

use std::time::Duration;

use aws_config::BehaviorVersion;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::ServerSideEncryption;
use sha2::{Digest, Sha256};
use tokio::sync::OnceCell;
use uuid::Uuid;

/// Hard ceiling on any single S3 operation. The compile finalise uploads
/// N+M blobs concurrently before opening the DB txn; without a per-op
/// timeout a hung/slow S3 endpoint would stall the compile past the task
/// queue's visibility timeout (60s), letting the reaper re-deliver the task
/// to a second worker while the first is still blocked. Reads (`get_blob`)
/// are on the semantic query hot path, so they're bounded too.
const BLOB_OP_TIMEOUT: Duration = Duration::from_secs(20);

/// Process-wide cached S3 client. `aws_config::load_defaults` walks the
/// credential chain (IMDS, env vars, shared config, pod identity) on
/// every call — ~tens of ms. The compile worker uploads N+M blobs per
/// revision and semantic queries fetch on every materialise; caching
/// the client makes those constant overhead. Safe under concurrent
/// initialisation: `OnceCell` serialises racing calls.
static S3_CLIENT: OnceCell<S3Client> = OnceCell::const_new();

const ENV_BUCKET: &str = "OXY_COMPILE_BLOB_S3_BUCKET";

/// Whether a custom S3 endpoint is configured (`AWS_ENDPOINT_URL`). Set for
/// S3-compatible stores — MinIO / LocalStack / Ceph — and unset for real AWS S3.
/// Drives two endpoint-specific behaviours: path-style addressing (below) and
/// skipping the forced SSE header (see [`force_sse_aes256`]).
fn has_custom_endpoint() -> bool {
    std::env::var("AWS_ENDPOINT_URL").is_ok_and(|v| !v.trim().is_empty())
}

/// Whether to set `ServerSideEncryption::AES256` on PutObject. Real AWS S3
/// applies SSE-S3 by default, so forcing the header is harmless defence-in-depth
/// there. But an S3-compatible store reached via a custom endpoint (MinIO without
/// KMS) rejects `x-amz-server-side-encryption: AES256` with `NotImplemented`,
/// failing EVERY upload — semantic blobs AND the DuckDB mirror, which silently
/// drops `s3_mirror` and leaves the stateless fleet with "no databases
/// configured". So only force it when talking to real AWS.
fn force_sse_aes256() -> bool {
    !has_custom_endpoint()
}

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
    // Sanitise `name` so a view/topic literally named `foo/bar` can't
    // restructure the S3 key path. Content addressing means this is not a
    // security/traversal concern, but flat names keep S3 listings tidy and
    // make per-name operations predictable. Replace any non
    // `[A-Za-z0-9_-]` character with `_`.
    let safe_name: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!(
        "workspaces/{workspace_id}/{prefix}/{safe_name}-{short_sha}.yml",
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
    let mut req = client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from(body.to_vec()))
        .content_type("application/yaml");
    if force_sse_aes256() {
        // Defense-in-depth: encrypt at rest even if the bucket lacks a default
        // SSE policy. Semantic bodies are business logic (table names, metric
        // definitions), so we don't rely on the bucket being configured right.
        // Skipped for custom-endpoint S3-compatible stores — see force_sse_aes256.
        req = req.server_side_encryption(ServerSideEncryption::Aes256);
    }
    let send = req.send();
    tokio::time::timeout(BLOB_OP_TIMEOUT, send)
        .await
        .map_err(|_| BlobError::Transport(format!("S3 put timed out after {BLOB_OP_TIMEOUT:?}")))?
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
    let send = client.get_object().bucket(&bucket).key(key).send();
    let resp = tokio::time::timeout(BLOB_OP_TIMEOUT, send)
        .await
        .map_err(|_| BlobError::Transport(format!("S3 get timed out after {BLOB_OP_TIMEOUT:?}")))?
        .map_err(|e| BlobError::Transport(format!("{e}")))?;
    let bytes = resp
        .body
        .collect()
        .await
        .map_err(|e| BlobError::Transport(format!("read body: {e}")))?
        .into_bytes();
    Ok(Some(bytes.to_vec()))
}

/// Whether an object already exists at `key`. Used to skip re-uploading
/// content-addressed data (the DuckDB mirror keys files by content hash, so an
/// unchanged file maps to the same key and need not be re-sent). `Ok(false)`
/// when no bucket is configured.
pub async fn object_exists(key: &str) -> Result<bool, BlobError> {
    let Some(bucket) = bucket() else {
        return Ok(false);
    };
    let client = s3_client().await;
    let send = client.head_object().bucket(&bucket).key(key).send();
    match tokio::time::timeout(BLOB_OP_TIMEOUT, send).await {
        Ok(Ok(_)) => Ok(true),
        // A 404 is the expected "not there yet" — only surface real transport
        // failures so the caller can decide whether to upload.
        Ok(Err(e)) => {
            let svc = e.into_service_error();
            if svc.is_not_found() {
                Ok(false)
            } else {
                Err(BlobError::Transport(format!("{svc}")))
            }
        }
        Err(_) => Err(BlobError::Transport(format!(
            "S3 head timed out after {BLOB_OP_TIMEOUT:?}"
        ))),
    }
}

/// Delete the object at `key`. `Ok(false)` when no bucket is configured, so a
/// single-node deployment is a no-op rather than an error.
///
/// Idempotent by design — S3 `DeleteObject` succeeds on a key that was never
/// there, and a caller deleting an artifact wants "it is gone", not "it was
/// there and now is not". This is the verb the store was missing: every write
/// path mirrored, and nothing removed, so a deleted result file kept being
/// served from the mirror to every replica.
pub async fn delete_object(key: &str) -> Result<bool, BlobError> {
    let Some(bucket) = bucket() else {
        return Ok(false);
    };
    let client = s3_client().await;
    let send = client.delete_object().bucket(&bucket).key(key).send();
    match tokio::time::timeout(BLOB_OP_TIMEOUT, send).await {
        Ok(Ok(_)) => Ok(true),
        Ok(Err(e)) => Err(BlobError::Transport(format!("{}", e.into_service_error()))),
        Err(_) => Err(BlobError::Transport(format!(
            "S3 delete timed out after {BLOB_OP_TIMEOUT:?}"
        ))),
    }
}

/// Upload `body` to an explicit `key` (unlike `put_blob`, which content-
/// addresses the key). Used by the DuckDB S3 mirror, whose keys follow a
/// `workspaces/{id}/duckdb/{revision}/…` scheme. `Ok(None)` when no bucket is
/// configured (caller treats that as "mirroring disabled"); `Ok(Some(key))` on
/// success. Encrypted at rest + bounded by the shared op timeout, like
/// `put_blob`.
pub async fn put_object_at_key(
    key: &str,
    body: Vec<u8>,
    content_type: &str,
) -> Result<Option<String>, BlobError> {
    let Some(bucket) = bucket() else {
        return Ok(None);
    };
    let client = s3_client().await;
    let mut req = client
        .put_object()
        .bucket(&bucket)
        .key(key)
        .body(ByteStream::from(body))
        .content_type(content_type.to_string());
    if force_sse_aes256() {
        // See force_sse_aes256: real AWS only — MinIO/LocalStack without KMS
        // reject the SSE header and the mirror upload fails, dropping s3_mirror.
        req = req.server_side_encryption(ServerSideEncryption::Aes256);
    }
    let send = req.send();
    tokio::time::timeout(BLOB_OP_TIMEOUT, send)
        .await
        .map_err(|_| BlobError::Transport(format!("S3 put timed out after {BLOB_OP_TIMEOUT:?}")))?
        .map_err(|e| BlobError::Transport(format!("{e}")))?;
    Ok(Some(key.to_string()))
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
            if has_custom_endpoint() {
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
