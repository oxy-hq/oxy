//! Per-workspace `OxyProjectContext` cache (§12 FU4c part 1).
//!
//! The cloud multi-tenant scheduler tick (§FU4b) rebuilds `WorkspaceManager`
//! + `OxyProjectContext` for every workspace, every interval — `N×` per
//! cycle. This cache memoizes a per-workspace handle by `Uuid` with a TTL
//! so repeated ticks reuse the built value. No size cap: workspace count
//! is bounded by the deployment; add an LRU here if it ever isn't.
//!
//! Staleness: TTL bounds how long a cached entry can lag a workspace's
//! on-disk config change. 10 min is a comfortable default for the periodic
//! tick (typically tens of seconds).
//!
//! The cache is generic over the cached type so unit tests don't need a
//! real `OxyProjectContext` (which requires a built `WorkspaceManager`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::agentic_wiring::OxyProjectContext;

const DEFAULT_TTL: Duration = Duration::from_secs(600);

pub struct TtlCache<V: Send + Sync + 'static> {
    ttl: Duration,
    inner: Mutex<HashMap<Uuid, Entry<V>>>,
}

pub type SemanticLayerCache = TtlCache<airlayer::SemanticLayer>;

pub fn new_semantic_layer_cache() -> Arc<SemanticLayerCache> {
    // 60s TTL: cheap safety net; explicit invalidation on every semantic file
    // write keeps the cache fresh within the same editing session.
    TtlCache::with_ttl(Duration::from_secs(60))
}

/// Caches the compiled `SemanticEngine` (join graph + evaluator) per workspace.
/// `SemanticEngine` is `Send` but not `Sync`; wrapping in `Mutex` makes it `Sync`.
/// Engine build cost (schema validation + join graph) is paid once per workspace
/// per TTL instead of once per compilation request.
pub type SemanticEngineCache = TtlCache<std::sync::Mutex<airlayer::SemanticEngine>>;

pub fn new_semantic_engine_cache() -> Arc<SemanticEngineCache> {
    // Explicit invalidation on every semantic file write is the primary freshness
    // mechanism (same as the layer cache). TTL is a safety net only — 10 min.
    TtlCache::with_ttl(Duration::from_secs(600))
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
    /// is idempotent for `OxyProjectContext`.
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

/// String-keyed result cache for expensive warehouse queries whose output is
/// stable enough to serve from memory for a few minutes (e.g. entity instance
/// pickers). Values are stored as raw JSON bytes to avoid importing domain types.
///
/// Bounded by MAX_ENTRIES to prevent unbounded growth under many concurrent
/// users or diverse key spaces. When the cap is hit, the oldest entry (by
/// insertion time) is evicted before inserting the new one.
pub struct QueryResultCache {
    ttl: Duration,
    max_entries: usize,
    inner: Mutex<std::collections::HashMap<String, (Instant, Vec<u8>)>>,
}

const DEFAULT_MAX_ENTRIES: usize = 500;

impl QueryResultCache {
    pub fn new(ttl: Duration, max_entries: usize) -> Arc<Self> {
        Arc::new(Self {
            ttl,
            max_entries,
            inner: Mutex::new(std::collections::HashMap::new()),
        })
    }

    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        let mut guard = self.inner.lock().expect("query result cache poisoned");
        let entry = guard.get(key)?;
        if entry.0.elapsed() >= self.ttl {
            guard.remove(key);
            return None;
        }
        Some(entry.1.clone())
    }

    pub fn insert(&self, key: String, value: Vec<u8>) {
        let mut guard = self.inner.lock().expect("query result cache poisoned");
        // Evict expired entries first; if still at cap, remove the oldest.
        if guard.len() >= self.max_entries {
            guard.retain(|_, v| v.0.elapsed() < self.ttl);
            if guard.len() >= self.max_entries
                && let Some(oldest_key) = guard
                    .iter()
                    .min_by_key(|(_, v)| v.0)
                    .map(|(k, _)| k.clone())
            {
                guard.remove(&oldest_key);
            }
        }
        guard.insert(key, (Instant::now(), value));
    }

    pub fn remove_prefix(&self, prefix: &str) {
        let mut guard = self.inner.lock().expect("query result cache poisoned");
        guard.retain(|k, _| !k.starts_with(prefix));
    }
}

pub fn new_query_result_cache() -> Arc<QueryResultCache> {
    QueryResultCache::new(Duration::from_secs(300), DEFAULT_MAX_ENTRIES)
}

pub type WorkspaceContextCache = TtlCache<OxyProjectContext>;

pub fn new_workspace_context_cache() -> Arc<WorkspaceContextCache> {
    TtlCache::with_ttl(DEFAULT_TTL)
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
