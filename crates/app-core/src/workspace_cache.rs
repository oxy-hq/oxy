//! Per-workspace TTL caches shared through `AppState`.
//!
//! `TtlCache<V>` is generic and depends on nothing app-specific, so it lives
//! here in `oxy-app-core` with `AppState`. The two aliases `AppState` actually
//! holds — [`SemanticLayerCache`] and [`SemanticEngineCache`] — are keyed on
//! `airlayer` types only.
//!
//! The `OxyProjectContext`-typed `WorkspaceContextCache` stays in `oxy-app`
//! (`server/router/workspace_cache.rs`): it needs the pipeline adapter, which is
//! oxy-app-internal, and `AppState` does not carry it.
//!
//! Staleness: TTL bounds how long a cached entry can lag a workspace's on-disk
//! config change. Explicit invalidation on every semantic file write is the
//! primary freshness mechanism; the TTL is a safety net.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use uuid::Uuid;

pub struct TtlCache<V: Send + Sync + 'static> {
    ttl: Duration,
    inner: Mutex<HashMap<Uuid, Entry<V>>>,
}

pub type SemanticLayerCache = TtlCache<oxy_airlayer_compat::SemanticLayer>;

pub fn new_semantic_layer_cache() -> Arc<SemanticLayerCache> {
    // 60s TTL: cheap safety net; explicit invalidation on every semantic file
    // write keeps the cache fresh within the same editing session.
    TtlCache::with_ttl(Duration::from_secs(60))
}

/// The compiled-engine cache is **not** a `TtlCache` alias any more: it lives in
/// `oxy-airlayer-compat` so `agentic-semantic` and `agentic-analytics` can reach
/// the same instance. Importing it from here would be a cargo cycle — this crate
/// already depends on `agentic-semantic`.
///
/// Re-exported under the old name so the eight `AppState` construction sites and
/// the `server::router::workspace_cache` shim are unchanged.
///
/// Two differences from the type this replaced, both deliberate:
///   * it is keyed on `(workspace_id, layer source, dialects)`, not
///     `workspace_id` alone — where the source says whether the layer came from
///     this node's working copy or from a materialised compiled revision. See
///     `oxy_airlayer_compat::engine_cache`;
///   * it holds a bare `Arc<SemanticEngine>`. The old alias wrapped the engine in
///     a `Mutex` on the stated grounds that it is "`Send` but not `Sync`". It is
///     both, so the lock only serialised compiles that could have run in
///     parallel; `preagg_rebuild` had already been sharing a bare
///     `Arc<SemanticEngine>` across spawned tasks, which is only possible for a
///     `Sync` type.
pub use oxy_airlayer_compat::SemanticEngineCache;

pub fn new_semantic_engine_cache() -> Arc<SemanticEngineCache> {
    // Explicit invalidation on every semantic file write is the primary freshness
    // mechanism (same as the layer cache). TTL is a safety net only — 10 min.
    SemanticEngineCache::new()
}

struct Entry<V> {
    built_at: Instant,
    value: Arc<V>,
}

impl<V: Send + Sync + 'static> TtlCache<V> {
    pub fn with_ttl(ttl: Duration) -> Arc<Self> {
        Arc::new(Self {
            ttl,
            inner: Mutex::new(HashMap::new()),
        })
    }

    pub fn lookup(&self, key: Uuid) -> Option<Arc<V>> {
        let mut guard = self.inner.lock().expect("ttl cache mutex poisoned");
        let Some(entry) = guard.get(&key) else {
            return None;
        };
        if entry.built_at.elapsed() >= self.ttl {
            guard.remove(&key);
            return None;
        }
        Some(entry.value.clone())
    }

    pub fn remove(&self, key: Uuid) {
        let mut guard = self.inner.lock().expect("ttl cache mutex poisoned");
        guard.remove(&key);
    }

    pub fn insert(&self, key: Uuid, value: Arc<V>) {
        let mut guard = self.inner.lock().expect("ttl cache mutex poisoned");
        guard.insert(
            key,
            Entry {
                built_at: Instant::now(),
                value,
            },
        );
    }

    /// Look up; on miss run `build` outside the lock and (if it succeeds)
    /// insert. Concurrent misses are tolerated — last writer wins, which
    /// is idempotent for the cached value.
    pub async fn get_or_build<F, Fut>(&self, key: Uuid, build: F) -> Option<Arc<V>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Option<Arc<V>>>,
    {
        if let Some(v) = self.lookup(key) {
            return Some(v);
        }
        let v = build().await?;
        self.insert(key, v.clone());
        Some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn hit_within_ttl_avoids_rebuild() {
        let cache: Arc<TtlCache<u32>> = TtlCache::with_ttl(Duration::from_secs(60));
        let key = Uuid::new_v4();
        let calls = Arc::new(AtomicUsize::new(0));

        let v1 = {
            let c = calls.clone();
            cache
                .get_or_build(key, || async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Some(Arc::new(7u32))
                })
                .await
                .unwrap()
        };
        assert_eq!(*v1, 7);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // Second call hits the cache; builder must not run.
        let v2 = cache
            .get_or_build(key, || async {
                panic!("builder must not be invoked on cache hit");
            })
            .await
            .unwrap();
        assert!(Arc::ptr_eq(&v1, &v2), "same Arc returned");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn ttl_expiration_forces_rebuild() {
        let cache: Arc<TtlCache<u32>> = TtlCache::with_ttl(Duration::from_millis(40));
        let key = Uuid::new_v4();
        cache.insert(key, Arc::new(1));
        assert!(cache.lookup(key).is_some());
        tokio::time::sleep(Duration::from_millis(70)).await;
        assert!(cache.lookup(key).is_none(), "expired entry must be gone");
    }

    #[tokio::test]
    async fn distinct_keys_are_isolated() {
        let cache: Arc<TtlCache<u32>> = TtlCache::with_ttl(Duration::from_secs(60));
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        cache.insert(a, Arc::new(1));
        cache.insert(b, Arc::new(2));
        assert_eq!(*cache.lookup(a).unwrap(), 1);
        assert_eq!(*cache.lookup(b).unwrap(), 2);
    }

    #[tokio::test]
    async fn failed_build_is_not_cached() {
        let cache: Arc<TtlCache<u32>> = TtlCache::with_ttl(Duration::from_secs(60));
        let key = Uuid::new_v4();
        let calls = Arc::new(AtomicUsize::new(0));

        for _ in 0..3 {
            let c = calls.clone();
            let r = cache
                .get_or_build(key, || async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    None
                })
                .await;
            assert!(r.is_none());
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            3,
            "None results are not memoized, so each call re-runs the builder"
        );
    }
}
