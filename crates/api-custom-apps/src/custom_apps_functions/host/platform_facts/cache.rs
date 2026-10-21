//! The in-process cache in front of [`super::build_platform_facts`].
//!
//! One build reads every org on the deployment, and every caller in an
//! internal org gets the same answer: the internal orgs decide `internal` and
//! who is staff, and they come from the process environment, not the caller. So one
//! entry per key serves everyone, for [`TTL`]. A failed load is not stored,
//! so the next call tries again.
//!
//! Per process, like `result_cache`: each serve replica holds its own copy,
//! and a fresh replica builds once on its first call.

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

/// How long one build answers for.
pub const TTL: Duration = Duration::from_secs(5 * 60);

/// A map from key to the value loaded for it and when, each answering until
/// `ttl` has passed. The lock is never held across the load.
pub struct TtlCache<K, V> {
    ttl: Duration,
    entries: Mutex<HashMap<K, (Instant, Arc<V>)>>,
}

impl<K: Eq + Hash + Clone, V> TtlCache<K, V> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// The value stored for `key`, if it was stored less than `ttl` before `now`.
    pub fn get(&self, key: &K, now: Instant) -> Option<Arc<V>> {
        let entries = self.entries.lock();
        let (stored_at, value) = entries.get(key)?;
        (now.saturating_duration_since(*stored_at) < self.ttl).then(|| value.clone())
    }

    /// Store `value` for `key` as of `now`, dropping entries that have expired.
    pub fn put(&self, key: K, value: Arc<V>, now: Instant) {
        let mut entries = self.entries.lock();
        entries.retain(|_, (stored_at, _)| now.saturating_duration_since(*stored_at) < self.ttl);
        entries.insert(key, (now, value));
    }

    /// The stored value for `key`, or `load`'s, stored on success. Two callers
    /// that miss at once both load; the later store wins, and both answers are
    /// equally fresh.
    pub async fn get_or_load<F, Fut, E>(&self, key: K, now: Instant, load: F) -> Result<Arc<V>, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<V, E>>,
    {
        if let Some(hit) = self.get(&key, now) {
            return Ok(hit);
        }
        let value = Arc::new(load().await?);
        self.put(key, value.clone(), now);
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// A load that counts its calls and answers `answer`.
    async fn counted(loads: &AtomicUsize, answer: &str) -> Result<String, String> {
        loads.fetch_add(1, Ordering::SeqCst);
        Ok(answer.to_string())
    }

    #[tokio::test]
    async fn a_second_call_inside_the_ttl_does_not_load_again() {
        let cache = TtlCache::new(TTL);
        let loads = AtomicUsize::new(0);
        let t0 = Instant::now();
        let first = cache.get_or_load(8, t0, || counted(&loads, "a")).await;
        let just_inside = t0 + TTL - Duration::from_secs(1);
        let second = cache
            .get_or_load(8, just_inside, || counted(&loads, "b"))
            .await;
        assert_eq!(loads.load(Ordering::SeqCst), 1, "the second call was a hit");
        assert_eq!(first.unwrap().as_str(), "a");
        assert_eq!(
            second.unwrap().as_str(),
            "a",
            "the hit is the first load's value"
        );
    }

    #[tokio::test]
    async fn at_the_ttl_the_entry_is_loaded_again() {
        let cache = TtlCache::new(TTL);
        let loads = AtomicUsize::new(0);
        let t0 = Instant::now();
        cache
            .get_or_load(8, t0, || counted(&loads, "a"))
            .await
            .unwrap();
        let fresh = cache
            .get_or_load(8, t0 + TTL, || counted(&loads, "b"))
            .await
            .unwrap();
        assert_eq!(loads.load(Ordering::SeqCst), 2);
        assert_eq!(fresh.as_str(), "b");
    }

    #[tokio::test]
    async fn each_key_loads_once_on_its_own() {
        let cache = TtlCache::new(TTL);
        let loads = AtomicUsize::new(0);
        let t0 = Instant::now();
        for weeks in [8, 4, 8, 4] {
            cache
                .get_or_load(weeks, t0, || counted(&loads, "x"))
                .await
                .unwrap();
        }
        assert_eq!(loads.load(Ordering::SeqCst), 2, "one load per key");
    }

    #[tokio::test]
    async fn a_failed_load_is_not_stored() {
        let cache: TtlCache<u32, String> = TtlCache::new(TTL);
        let loads = AtomicUsize::new(0);
        let t0 = Instant::now();
        let failed = cache
            .get_or_load(8, t0, || async {
                loads.fetch_add(1, Ordering::SeqCst);
                Err::<String, _>("db down".to_string())
            })
            .await;
        assert!(failed.is_err());
        let retried = cache.get_or_load(8, t0, || counted(&loads, "a")).await;
        assert_eq!(
            loads.load(Ordering::SeqCst),
            2,
            "the failure was not cached"
        );
        assert_eq!(retried.unwrap().as_str(), "a");
    }

    #[test]
    fn an_expired_entry_is_dropped_when_another_is_stored() {
        let cache = TtlCache::new(TTL);
        let t0 = Instant::now();
        cache.put(4, Arc::new("old"), t0);
        cache.put(8, Arc::new("new"), t0 + TTL);
        assert_eq!(cache.entries.lock().len(), 1, "the expired entry went");
        assert!(cache.get(&4, t0 + TTL).is_none());
    }
}
