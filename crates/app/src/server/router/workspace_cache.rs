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
