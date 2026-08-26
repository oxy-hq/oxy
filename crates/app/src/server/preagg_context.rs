//! Builds the [`PreaggContext`] the compile path takes, including where to
//! read a rollup this node did not build.
//!
//! There is no download here and no async bridge, which is the point. The
//! rebuild mirrors each rollup Parquet to the compile-boundary blob bucket;
//! a node that never built it points DuckDB at the `s3://` object instead of
//! fetching it, exactly as `connector::duckdb` does for an S3-mirrored
//! local-file warehouse. The compile path stays synchronous and this module
//! stays a config lookup.

use std::sync::Arc;

use agentic_semantic::compile::{BlobConfig, PreaggContext};

/// Where this process's rollup objects live, or `None` when no blob bucket is
/// configured — in which case the local file is the only copy and a node that
/// did not build a rollup falls back to the warehouse.
///
/// Region and endpoint come from the same environment the AWS SDK reads, so a
/// MinIO / LocalStack dev box needs no extra configuration to work here.
pub fn blob_config() -> Option<BlobConfig> {
    Some(BlobConfig {
        bucket: oxy_compile::blob_store::bucket()?,
        region: env_non_empty("AWS_REGION").or_else(|| env_non_empty("AWS_DEFAULT_REGION")),
        endpoint_url: env_non_empty("AWS_ENDPOINT_URL"),
    })
}

fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Assemble the rollup short-circuit for one request.
///
/// `None` when the node has no Layer-1 cache (and therefore no rebuild
/// worker): without one there is no guarantee a rollup is current, so the
/// query compiles to warehouse SQL — the same posture the CLI and the builder
/// validator take.
///
/// `renewal_threshold_secs` comes from the workspace's own `preagg:` block,
/// resolved per request (see `workspace_context`), not from a process-wide
/// default.
pub fn preagg_context(
    workspace_id: uuid::Uuid,
    cache: Option<Arc<std::sync::RwLock<agentic_semantic::refresh_key_cache::RefreshKeyCache>>>,
    renewal_threshold_secs: Option<u64>,
) -> Option<PreaggContext> {
    Some(PreaggContext {
        workspace_id,
        cache: cache?,
        renewal_threshold_secs: renewal_threshold_secs
            .unwrap_or(oxy::config::preagg_check::DEFAULT_RENEWAL_SECS),
        blob: blob_config(),
    })
}

#[cfg(test)]
mod tests {
    use agentic_semantic::compile::{BlobConfig, PreaggContext, PreaggSource, try_resolve_preagg};

    /// Removes the manifest this test writes even when an assertion panics —
    /// the cache dir is the real process-wide state dir, so a cleanup on the
    /// happy path only would leave debris in a developer's `~/.local/share/oxy`
    /// on exactly the runs they're debugging.
    struct CacheDirGuard(std::path::PathBuf);
    impl Drop for CacheDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The read side and the write side address the same object.
    ///
    /// `agentic-semantic` builds the `s3://` URI it reads from, and
    /// `oxy-compile::preagg_blob` builds the key it mirrors to. Neither crate
    /// can see the other — that separation is deliberate — so this is the one
    /// place both are in scope, and the only thing holding them together. If
    /// it fails, every cross-node rollup read 404s while both sides look
    /// individually correct.
    #[test]
    fn the_uri_the_read_path_builds_matches_the_key_the_mirror_writes() {
        let workspace_id = uuid::Uuid::from_u128(1234);
        let cache_dir = oxy::state_dir::get_airlayer_cache_dir(workspace_id);
        std::fs::create_dir_all(&cache_dir).expect("cache dir");
        let _cleanup = CacheDirGuard(cache_dir.clone());
        // A manifest listing one rollup, with no Parquet beside it — the state
        // of every node that did not run the rebuild.
        std::fs::write(
            cache_dir.join("manifest.json"),
            serde_json::json!({
                "pulled_at": "2026-08-25T00:00:00Z",
                "source_database": "local",
                "rollups": [{
                    "view_name": "orders",
                    "rollup_name": "by_month",
                    "rollup_hash": "deadbeef",
                    "file": "orders__deadbeef.parquet",
                    "dimensions": [],
                    "measures": [{"name": "total_orders", "type": "count"}],
                    "time_dimension": null,
                    "granularity": null,
                    "build_date": "2026-08-25 00:00:00"
                }]
            })
            .to_string(),
        )
        .expect("manifest writes");

        let request = serde_json::from_value(serde_json::json!({
            "measures": ["orders.total_orders"],
            "dimensions": []
        }))
        .expect("request parses");

        let preagg = PreaggContext {
            workspace_id,
            cache: std::sync::Arc::new(std::sync::RwLock::new(
                agentic_semantic::refresh_key_cache::RefreshKeyCache::new(),
            )),
            renewal_threshold_secs: 0,
            blob: Some(BlobConfig {
                bucket: "oxy-blobs".to_string(),
                region: None,
                endpoint_url: None,
            }),
        };
        let compiled = try_resolve_preagg(&preagg, &request, "SELECT 'unused'", "local")
            .expect("the blob tier resolves");
        let agentic_semantic::compile::CompiledQuery::Preaggregation { source, .. } = compiled
        else {
            panic!("expected a rollup resolution");
        };
        let PreaggSource::Blob { uri, .. } = &source else {
            panic!("expected a blob source, got {source:?}");
        };

        let mirror_key = oxy_compile::preagg_blob::parquet_key(
            &oxy::state_dir::airlayer_cache_key(workspace_id),
            "orders__deadbeef.parquet",
        );
        assert_eq!(uri, &format!("s3://oxy-blobs/{mirror_key}"));
    }
}
