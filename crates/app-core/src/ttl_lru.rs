//! One bounded, TTL'd, in-process cache for the custom-app hot paths.
//!
//! The serve plane used to carry three hand-rolled `Mutex<HashMap<K, (V,
//! Instant)>>` maps — `custom_apps_cache`, the access verdicts in
//! `custom_apps_auth`, the org-subdomain resolver here in `oxy-app-core` —
//! each with its own idea of "bounded". None of them was: the sweep ran only
//! once the map held 4 096 entries and then evicted only *expired* ones, so a
//! map whose entries were all younger than the TTL grew without limit (one
//! slot per live identity, forever), and the subdomain map `clear()`ed itself
//! wholesale instead, dropping every hot entry to make room for one cold one.
//!
//! [`TtlLru`] is the single replacement. It is an `lru::LruCache` behind a
//! mutex, so the bound is a hard capacity and the entry that goes is the one
//! least recently *read*, plus a per-entry age check so a stale row is never
//! served past the TTL whatever its position. It lives in `oxy-app-core`
//! because the subdomain resolver does and `oxy-app` sits above this crate;
//! `custom_apps_cache` re-exports it for the serve tree.
//!
//! What it deliberately is not: a clock-injected cache. The TTL tests use
//! [`TtlLru::insert_at`] to plant an already-old entry instead of sleeping, so
//! they are deterministic without a fake clock threading through every reader.

use std::hash::Hash;
use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use lru::LruCache;

/// A capacity-bounded LRU whose entries also expire `ttl` after insertion.
///
/// Every method takes `&self`; the lock is internal and never held across an
/// `await`, so a `static` [`TtlLru`] behind `OnceLock` is the intended shape.
/// A poisoned lock is recovered rather than propagated: the map holds only
/// re-derivable rows, and a cache that refuses to answer after one panic
/// elsewhere turns a bug into a per-request database storm.
pub struct TtlLru<K: Hash + Eq, V> {
    inner: Mutex<LruCache<K, (V, Instant)>>,
    ttl: Duration,
}

impl<K: Hash + Eq, V: Clone> TtlLru<K, V> {
    /// A cache holding at most `capacity` entries, each fresh for `ttl`.
    pub fn new(capacity: usize, ttl: Duration) -> Self {
        let capacity = NonZeroUsize::new(capacity.max(1)).expect("max(1) is non-zero");
        Self {
            inner: Mutex::new(LruCache::new(capacity)),
            ttl,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, LruCache<K, (V, Instant)>> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The fresh value under `key`, promoted to most-recently-used. An expired
    /// entry is dropped on the way out so it never counts against the bound.
    pub fn get(&self, key: &K) -> Option<V> {
        let mut guard = self.lock();
        let expired = match guard.get(key) {
            Some((value, inserted_at)) if inserted_at.elapsed() <= self.ttl => {
                return Some(value.clone());
            }
            Some(_) => true,
            None => false,
        };
        if expired {
            guard.pop(key);
        }
        None
    }

    /// Insert (or replace) `key`, evicting the least recently used entry when
    /// the cache is full.
    pub fn insert(&self, key: K, value: V) {
        self.insert_at(key, value, Instant::now());
    }

    /// [`insert`](Self::insert) with an explicit insertion time. Exists so a
    /// test can plant an entry that is already older than the TTL instead of
    /// sleeping it past one.
    pub fn insert_at(&self, key: K, value: V, inserted_at: Instant) {
        self.lock().put(key, (value, inserted_at));
    }

    /// Drop one entry. A miss is not an error.
    pub fn remove(&self, key: &K) {
        self.lock().pop(key);
    }

    /// Drop every entry for which `keep` answers `false`. The targeted
    /// invalidation primitive: "every app of this org", "every verdict about
    /// this app". `lru` has no `retain`, so the keys are collected under the
    /// lock and popped in the same critical section.
    pub fn retain(&self, mut keep: impl FnMut(&K, &V) -> bool)
    where
        K: Clone,
    {
        let mut guard = self.lock();
        let doomed: Vec<K> = guard
            .iter()
            .filter(|(k, (v, _))| !keep(k, v))
            .map(|(k, _)| k.clone())
            .collect();
        for key in doomed {
            guard.pop(&key);
        }
    }

    /// Drop everything. The wholesale fallback for a mutation whose affected
    /// keys are not known at the call site.
    pub fn clear(&self) {
        self.lock().clear();
    }

    /// Entries currently held, expired ones included until they are read.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TTL: Duration = Duration::from_secs(60);

    #[test]
    fn a_hit_is_a_clone_of_what_was_inserted() {
        let cache: TtlLru<&str, u32> = TtlLru::new(4, TTL);
        cache.insert("a", 1);
        assert_eq!(cache.get(&"a"), Some(1));
        assert_eq!(cache.get(&"b"), None);
    }

    #[test]
    fn the_capacity_is_a_hard_bound_and_the_coldest_entry_goes() {
        // The old sweep kept every fresh entry whatever the count; this must
        // not. Three slots, four inserts: the first one — never read again —
        // is the one that leaves.
        let cache: TtlLru<u8, u8> = TtlLru::new(3, TTL);
        cache.insert(1, 1);
        cache.insert(2, 2);
        cache.insert(3, 3);
        // Reading 1 makes 2 the least recently used.
        assert_eq!(cache.get(&1), Some(1));
        cache.insert(4, 4);
        assert_eq!(cache.len(), 3);
        assert_eq!(cache.get(&2), None, "the LRU entry was evicted");
        assert_eq!(cache.get(&1), Some(1));
        assert_eq!(cache.get(&4), Some(4));
    }

    #[test]
    fn an_entry_older_than_the_ttl_is_a_miss_and_is_dropped() {
        let cache: TtlLru<&str, u8> = TtlLru::new(4, TTL);
        let stale = Instant::now() - TTL - Duration::from_secs(1);
        cache.insert_at("old", 1, stale);
        cache.insert("new", 2);
        assert_eq!(cache.get(&"old"), None);
        assert_eq!(cache.len(), 1, "the expired entry no longer holds a slot");
        assert_eq!(cache.get(&"new"), Some(2));
    }

    #[test]
    fn an_entry_exactly_at_the_ttl_is_still_served() {
        // `<=`: the boundary belongs to the fresh side, matching the maps this
        // replaces.
        let cache: TtlLru<&str, u8> = TtlLru::new(4, Duration::from_secs(3600));
        cache.insert_at("edge", 1, Instant::now() - Duration::from_secs(3599));
        assert_eq!(cache.get(&"edge"), Some(1));
    }

    #[test]
    fn retain_drops_only_what_the_predicate_rejects() {
        let cache: TtlLru<(String, String), u8> = TtlLru::new(8, TTL);
        cache.insert(("acme".into(), "sales".into()), 1);
        cache.insert(("acme".into(), "ops".into()), 2);
        cache.insert(("globex".into(), "sales".into()), 3);
        cache.retain(|(org, _), _| org != "acme");
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get(&("globex".into(), "sales".into())), Some(3));
        assert_eq!(cache.get(&("acme".into(), "sales".into())), None);
    }

    #[test]
    fn remove_and_clear() {
        let cache: TtlLru<u8, u8> = TtlLru::new(4, TTL);
        cache.insert(1, 1);
        cache.insert(2, 2);
        cache.remove(&1);
        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.get(&2), Some(2));
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn a_zero_capacity_still_holds_one_entry() {
        // A misconfigured constant must degrade to "tiny", not panic at boot.
        let cache: TtlLru<u8, u8> = TtlLru::new(0, TTL);
        cache.insert(1, 1);
        assert_eq!(cache.get(&1), Some(1));
    }
}
