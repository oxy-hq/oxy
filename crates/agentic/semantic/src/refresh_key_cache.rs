use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Cached result of a single refresh_key evaluation.
#[derive(Debug, Clone)]
pub struct RefreshKeyCacheEntry {
    /// Last evaluated value (None for `Every`-based keys).
    pub value: Option<String>,
    /// When this entry was last written.
    pub checked_at: Instant,
}

/// In-process cache for refresh_key results.
///
/// Keyed by `rollup_hash`. Prevents a warehouse round-trip on every
/// semantic query by memoising the last-evaluated refresh_key result
/// for `renewal_threshold` seconds.
#[derive(Debug, Default)]
pub struct RefreshKeyCache {
    entries: HashMap<String, RefreshKeyCacheEntry>,
}

impl RefreshKeyCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the cached entry for `rollup_hash` if it was checked within `threshold`.
    pub fn get(&self, rollup_hash: &str, threshold: Duration) -> Option<&RefreshKeyCacheEntry> {
        self.entries
            .get(rollup_hash)
            .filter(|e| e.checked_at.elapsed() < threshold)
    }

    /// Store a fresh evaluation result.
    pub fn insert(&mut self, rollup_hash: String, value: Option<String>) {
        self.entries.insert(
            rollup_hash,
            RefreshKeyCacheEntry {
                value,
                checked_at: Instant::now(),
            },
        );
    }

    /// Remove a specific entry (called by the background worker after rebuilding).
    pub fn invalidate(&mut self, rollup_hash: &str) {
        self.entries.remove(rollup_hash);
    }

    /// Remove all entries older than `max_age`.
    ///
    /// Call this at the start of each background cycle to prevent unbounded
    /// growth when rollups are renamed or removed from views.
    pub fn sweep(&mut self, max_age: Duration) {
        self.entries.retain(|_, e| e.checked_at.elapsed() < max_age);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_miss_when_empty() {
        let cache = RefreshKeyCache::new();
        assert!(cache.get("abc123", Duration::from_secs(120)).is_none());
    }

    #[test]
    fn test_cache_hit_within_threshold() {
        let mut cache = RefreshKeyCache::new();
        cache.insert("abc123".into(), Some("42".into()));
        let entry = cache.get("abc123", Duration::from_secs(120));
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().value.as_deref(), Some("42"));
    }

    #[test]
    fn test_cache_miss_after_threshold() {
        let mut cache = RefreshKeyCache::new();
        cache.insert("abc123".into(), Some("42".into()));
        // threshold of 0 → immediately expired
        let entry = cache.get("abc123", Duration::from_secs(0));
        assert!(entry.is_none());
    }

    #[test]
    fn test_invalidate_removes_entry() {
        let mut cache = RefreshKeyCache::new();
        cache.insert("abc123".into(), Some("42".into()));
        cache.invalidate("abc123");
        assert!(cache.get("abc123", Duration::from_secs(120)).is_none());
    }

    #[test]
    fn test_sweep_removes_old_entries() {
        let mut cache = RefreshKeyCache::new();
        cache.insert("abc".into(), Some("v1".into()));
        cache.insert("def".into(), Some("v2".into()));
        // threshold of 0 → both entries are immediately expired
        cache.sweep(Duration::from_secs(0));
        assert!(cache.get("abc", Duration::from_secs(120)).is_none());
        assert!(cache.get("def", Duration::from_secs(120)).is_none());
    }

    #[test]
    fn test_sweep_keeps_fresh_entries() {
        let mut cache = RefreshKeyCache::new();
        cache.insert("abc".into(), Some("v1".into()));
        // max_age of 120s → entry is fresh, should survive
        cache.sweep(Duration::from_secs(120));
        assert!(cache.get("abc", Duration::from_secs(120)).is_some());
    }
}
