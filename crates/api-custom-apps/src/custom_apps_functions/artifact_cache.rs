//! In-process cache of function artifacts (`functions/<name>.js`).
//!
//! Keyed by `(app_id, build_id, artifact_rel)`. A build is immutable — an
//! `app_builds (app_id, build_id)` row is written once by a publish and never
//! rewritten — so an entry can never go stale: a new publish is a new
//! `build_id`, which is a new key. Until this existed every invocation fetched
//! its module from the build store, so a schedule firing every five minutes
//! paid one S3 GET per tick for bytes that had not changed since it was
//! published.
//!
//! Bounded by bytes, not entries: artifacts range from a few KB to a bundled
//! megabyte, and the least recently used module goes first.
//!
//! Single-flight: N concurrent misses on one key issue one build-store GET and
//! share its bytes. Without it a page calling three functions at once on a
//! cold replica, or a fleet draining a backlog of one schedule, paid one round
//! trip per call.
//!
//! Only a present artifact is remembered. A miss (`Ok(None)`) and a store
//! error reach every waiter and are cached by nobody, so a transient S3
//! failure, or a build whose upload is still landing, is retried on the next
//! call rather than pinned for the process lifetime.

use std::collections::HashMap;
use std::future::Future;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use axum::body::Bytes;
use lru::LruCache;
use tokio::sync::OnceCell;
use uuid::Uuid;

use super::super::custom_apps_build_store::{self, BuildStoreError};

/// Bytes the process-wide cache may hold. 64 MiB is a few hundred bundled
/// modules — more builds than one replica serves live.
const BUDGET_BYTES: usize = 64 * 1024 * 1024;
/// Entry ceiling behind the byte budget, so a flood of tiny modules cannot
/// make the LRU's bookkeeping the cost.
const MAX_ENTRIES: usize = 4096;

type Key = (Uuid, String, String);
type Fetched<E> = Result<Option<Bytes>, E>;
/// One in-flight fetch, awaited by every caller that missed while it ran.
type Flight<E> = Arc<OnceCell<Fetched<E>>>;

pub(super) struct ArtifactCache<E> {
    state: Mutex<State<E>>,
    budget_bytes: usize,
}

struct State<E> {
    entries: LruCache<Key, Bytes>,
    used_bytes: usize,
    in_flight: HashMap<Key, Flight<E>>,
}

impl<E: Clone> ArtifactCache<E> {
    pub(super) fn new(budget_bytes: usize, max_entries: usize) -> Self {
        Self {
            state: Mutex::new(State {
                entries: LruCache::new(NonZeroUsize::new(max_entries).expect("max_entries > 0")),
                used_bytes: 0,
                in_flight: HashMap::new(),
            }),
            budget_bytes,
        }
    }

    /// The artifact under `key`: from the cache, or from one shared `fetch`.
    pub(super) async fn get_or_fetch<F, Fut>(&self, key: Key, fetch: F) -> Fetched<E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Fetched<E>>,
    {
        let flight = {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(bytes) = state.entries.get(&key) {
                return Ok(Some(bytes.clone()));
            }
            Arc::clone(state.in_flight.entry(key.clone()).or_default())
        };
        // The first caller to get here runs `fetch`; the rest await its cell.
        // A waiter whose initializer is cancelled mid-fetch hands the cell to
        // the next one, which runs its own `fetch` — so no miss is left hanging.
        let result = flight.get_or_init(fetch).await.clone();
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        // Whichever waiter settles first retires the flight and admits the
        // bytes; the rest find it gone. `ptr_eq` so a flight a later miss
        // opened under the same key is left to its own waiters.
        if state
            .in_flight
            .get(&key)
            .is_some_and(|open| Arc::ptr_eq(open, &flight))
        {
            state.in_flight.remove(&key);
            if let Ok(Some(bytes)) = &result {
                state.admit(key, bytes.clone(), self.budget_bytes);
            }
        }
        result
    }

    #[cfg(test)]
    fn used_bytes(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .used_bytes
    }

    #[cfg(test)]
    fn contains(&self, key: &Key) -> bool {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entries
            .contains(key)
    }
}

impl<E> State<E> {
    /// Remember `bytes` under `key`, evicting least recently used entries
    /// until it fits. An artifact larger than the whole budget is served and
    /// not remembered, rather than emptying the cache for one module.
    fn admit(&mut self, key: Key, bytes: Bytes, budget_bytes: usize) {
        let len = bytes.len();
        if len > budget_bytes {
            return;
        }
        while self.used_bytes + len > budget_bytes {
            match self.entries.pop_lru() {
                Some((_, evicted)) => self.used_bytes -= evicted.len(),
                None => break,
            }
        }
        self.used_bytes += len;
        // `push` returns the entry it displaced: the previous value under this
        // key, or the LRU when the entry ceiling is hit.
        if let Some((_, displaced)) = self.entries.push(key, bytes) {
            self.used_bytes -= displaced.len();
        }
    }
}

fn global() -> &'static ArtifactCache<BuildStoreError> {
    static CACHE: OnceLock<ArtifactCache<BuildStoreError>> = OnceLock::new();
    CACHE.get_or_init(|| ArtifactCache::new(BUDGET_BYTES, MAX_ENTRIES))
}

/// The build-store object `artifact_rel` of build `build_id` of `app_id`,
/// through the process-wide cache. Same contract as
/// [`custom_apps_build_store::get_object`]: `Ok(None)` when absent.
pub(super) async fn get(
    app_id: Uuid,
    build_id: &str,
    artifact_rel: &str,
) -> Result<Option<Bytes>, BuildStoreError> {
    let key = (app_id, build_id.to_string(), artifact_rel.to_string());
    let (build_id, artifact_rel) = (key.1.clone(), key.2.clone());
    global()
        .get_or_fetch(key, move || async move {
            custom_apps_build_store::get_object(app_id, &build_id, &artifact_rel).await
        })
        .await
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::sync::Notify;

    use super::*;

    fn key(n: u8) -> Key {
        (
            Uuid::from_u128(1),
            "b1".to_string(),
            format!("functions/f{n}.js"),
        )
    }

    fn bytes(len: usize) -> Bytes {
        Bytes::from(vec![b'x'; len])
    }

    #[tokio::test]
    async fn a_second_read_is_a_hit_and_fetches_nothing() {
        let cache: ArtifactCache<String> = ArtifactCache::new(1024, 16);
        let fetches = AtomicUsize::new(0);
        let fetch = || async {
            fetches.fetch_add(1, Ordering::SeqCst);
            Ok(Some(bytes(10)))
        };
        let first = cache.get_or_fetch(key(1), fetch).await.unwrap();
        let second = cache.get_or_fetch(key(1), fetch).await.unwrap();
        assert_eq!(first, second);
        assert_eq!(
            fetches.load(Ordering::SeqCst),
            1,
            "the second read was a hit"
        );
        assert_eq!(cache.used_bytes(), 10);
    }

    #[tokio::test]
    async fn a_different_key_is_a_miss() {
        let cache: ArtifactCache<String> = ArtifactCache::new(1024, 16);
        let fetches = AtomicUsize::new(0);
        let fetch = || async {
            fetches.fetch_add(1, Ordering::SeqCst);
            Ok(Some(bytes(10)))
        };
        cache.get_or_fetch(key(1), fetch).await.unwrap();
        cache.get_or_fetch(key(2), fetch).await.unwrap();
        let other_build = (
            Uuid::from_u128(1),
            "b2".to_string(),
            "functions/f1.js".to_string(),
        );
        cache.get_or_fetch(other_build, fetch).await.unwrap();
        assert_eq!(fetches.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn an_absent_artifact_and_a_store_error_are_not_remembered() {
        let cache: ArtifactCache<String> = ArtifactCache::new(1024, 16);
        let fetches = AtomicUsize::new(0);
        let missing = || async {
            fetches.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        };
        assert_eq!(cache.get_or_fetch(key(1), missing).await, Ok(None));
        assert_eq!(cache.get_or_fetch(key(1), missing).await, Ok(None));
        let failing = || async {
            fetches.fetch_add(1, Ordering::SeqCst);
            Err("s3 down".to_string())
        };
        assert_eq!(
            cache.get_or_fetch(key(2), failing).await,
            Err("s3 down".to_string())
        );
        assert_eq!(
            cache.get_or_fetch(key(2), failing).await.unwrap_err(),
            "s3 down"
        );
        assert_eq!(fetches.load(Ordering::SeqCst), 4, "every call fetched");
        assert!(!cache.contains(&key(1)) && !cache.contains(&key(2)));
        assert_eq!(cache.used_bytes(), 0);
    }

    #[tokio::test]
    async fn the_byte_budget_evicts_least_recently_used_first() {
        let cache: ArtifactCache<String> = ArtifactCache::new(100, 16);
        for n in 1..=3 {
            cache
                .get_or_fetch(key(n), || async { Ok(Some(bytes(40))) })
                .await
                .unwrap();
        }
        // 1 was evicted to fit 3 (40 + 40 + 40 > 100); 2 and 3 remain.
        assert!(!cache.contains(&key(1)));
        assert!(cache.contains(&key(2)) && cache.contains(&key(3)));
        assert_eq!(cache.used_bytes(), 80);
        // Touch 2 so 3 is the LRU, then admit a 50-byte entry: 3 goes.
        cache
            .get_or_fetch(key(2), || async { panic!("hit expected") })
            .await
            .unwrap();
        cache
            .get_or_fetch(key(4), || async { Ok(Some(bytes(50))) })
            .await
            .unwrap();
        assert!(cache.contains(&key(2)) && cache.contains(&key(4)));
        assert!(!cache.contains(&key(3)));
        assert_eq!(cache.used_bytes(), 90);
    }

    #[tokio::test]
    async fn an_artifact_over_the_whole_budget_is_served_but_not_kept() {
        let cache: ArtifactCache<String> = ArtifactCache::new(100, 16);
        cache
            .get_or_fetch(key(1), || async { Ok(Some(bytes(40))) })
            .await
            .unwrap();
        let big = cache
            .get_or_fetch(key(2), || async { Ok(Some(bytes(101))) })
            .await
            .unwrap();
        assert_eq!(big.map(|b| b.len()), Some(101));
        assert!(cache.contains(&key(1)), "the budget was not emptied for it");
        assert!(!cache.contains(&key(2)));
    }

    #[tokio::test]
    async fn concurrent_misses_on_one_key_fetch_once() {
        let cache: Arc<ArtifactCache<String>> = Arc::new(ArtifactCache::new(1024, 16));
        let fetches = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(Notify::new());
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let (cache, fetches, release) = (cache.clone(), fetches.clone(), release.clone());
            tasks.push(tokio::spawn(async move {
                cache
                    .get_or_fetch(key(1), || async move {
                        fetches.fetch_add(1, Ordering::SeqCst);
                        // Hold the fetch open until every task has queued up.
                        release.notified().await;
                        Ok(Some(bytes(7)))
                    })
                    .await
            }));
        }
        // Let every task reach the flight, then release the one fetch.
        tokio::task::yield_now().await;
        while fetches.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        release.notify_waiters();
        release.notify_one();
        for task in tasks {
            assert_eq!(task.await.unwrap().unwrap().map(|b| b.len()), Some(7));
        }
        assert_eq!(
            fetches.load(Ordering::SeqCst),
            1,
            "one GET served eight misses"
        );
        assert_eq!(cache.used_bytes(), 7);
    }
}
