//! Cross-node read-through for the pre-aggregation cache.
//!
//! A rollup rebuild writes its Parquet + manifest to the *building* node's
//! local disk (`state_dir/airlayer/cache/<key>`, `<key>` = the workspace id).
//! On a multi-instance fleet, a LATER query — or the status
//! tab — can land on a DIFFERENT node, which has never built anything and
//! sees a bare local directory. Same shape as `duckdb_mirror` and the
//! `runtime_artifact` module in `oxy-app`: write locally, best-effort mirror
//! to S3; on a local miss, read the object straight out of S3 rather than
//! copying it down.
//!
//! Reuses the compile-boundary blob bucket (`OXY_COMPILE_BLOB_S3_BUCKET`) via
//! [`crate::blob_store`] — no new bucket, no new env var. Unconfigured →
//! every call here is a no-op and behavior is byte-identical to a single-node
//! deployment: the local file is the only copy.
//!
//! **This module is the WRITE half only.** The manifest is small and is read
//! back through [`fetch_manifest`], but a rollup Parquet is never fetched:
//! DuckDB reads it in place over `httpfs` from an `s3://` URI
//! (`agentic_semantic::compile::PreaggSource::Blob`), which skips the download,
//! the staging file, and the local copy entirely, and lets DuckDB push
//! projections and filters into the scan. [`parquet_key`] is public so the
//! read side can address the same object.
//!
//! `cache_key` is the caller's local cache-directory name — the workspace id,
//! see `oxy_shared::state_dir::airlayer_cache_key`; passed in rather than
//! recomputed here so both sides derive it from the SAME source of truth
//! without this module depending on `oxy-shared`.
//!
//! It is a workspace id and not a path hash for a reason that bites hardest
//! here: these objects share one multi-tenant bucket under a common
//! `runtime/preagg/` prefix, so the key is the tenant boundary. A hash of a
//! filesystem path is not that — it changes with the checkout (a
//! `.worktrees/<branch>` request keyed to a different directory than the
//! rebuild wrote), and it names a node's disk layout rather than the workspace
//! whose data is inside.

use crate::blob_store;

fn manifest_key(cache_key: &str) -> String {
    format!("runtime/preagg/{cache_key}/manifest.json")
}

/// The object key a rollup Parquet is mirrored to.
///
/// Public because the READ side no longer goes through this module: DuckDB
/// reads the object in place over `httpfs`, so `agentic-semantic` builds the
/// same `s3://` URI from its own copy of this shape. Two crates that can't see
/// each other now derive one key — `compile::the_blob_key_matches_what_the_mirror_writes`
/// is what holds them together.
pub fn parquet_key(cache_key: &str, file_name: &str) -> String {
    format!("runtime/preagg/{cache_key}/{file_name}")
}

/// Mirror a just-written manifest.json. Best-effort: a failure only costs the
/// cross-node read fallback (logs and leaves the old S3 copy, or none, in
/// place), never the caller's write, which already succeeded locally.
pub async fn mirror_manifest(cache_key: &str, bytes: Vec<u8>) {
    mirror(&manifest_key(cache_key), bytes, "application/json").await;
}

/// Mirror a just-written rollup Parquet file.
pub async fn mirror_parquet(cache_key: &str, file_name: &str, bytes: Vec<u8>) {
    mirror(
        &parquet_key(cache_key, file_name),
        bytes,
        "application/vnd.apache.parquet",
    )
    .await;
}

/// Read-through fetch for a manifest.json, used only on a local miss.
pub async fn fetch_manifest(cache_key: &str) -> Option<Vec<u8>> {
    fetch(&manifest_key(cache_key)).await
}

async fn mirror(key: &str, bytes: Vec<u8>, content_type: &str) {
    match blob_store::put_object_at_key(key, bytes, content_type).await {
        Ok(Some(_)) => tracing::debug!(key, "preagg artifact mirrored to S3"),
        Ok(None) => {} // no bucket configured — local disk is the only copy
        Err(e) => tracing::warn!(
            error = %e,
            key,
            "preagg artifact S3 mirror failed (best-effort; cross-node reads of \
             this rollup will miss until the next rebuild)"
        ),
    }
}

async fn fetch(key: &str) -> Option<Vec<u8>> {
    match blob_store::get_blob(key).await {
        Ok(Some(bytes)) => Some(bytes),
        Ok(None) => None,
        Err(e) => {
            // A genuine miss surfaces here too (NoSuchKey is a transport error
            // in this client); either way there's nothing to serve, so this is
            // the expected shape of "never built anywhere", not necessarily a
            // fault — debug, not warn.
            tracing::debug!(error = %e, key, "preagg artifact S3 read-through miss");
            None
        }
    }
}
