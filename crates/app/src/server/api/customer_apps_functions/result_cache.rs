//! Opt-in result cache for Oxy Functions (manifest `cache: { ttlSeconds }`).
//!
//! A function is arbitrary, frequently side-effectful server-side logic, so
//! results are NEVER cached by default — only functions that explicitly declare
//! a cache TTL land here. The key is:
//!
//!   (build_id, function_name, user_id, hash(request_body))
//!
//! - `build_id` — a promote/rollback rotates the channel pointer to a new
//!   build, so the cache invalidates automatically on deploy (no eviction).
//! - `user_id` — USER-SCOPED: a function receives `ctx.user` and may return
//!   per-user data, so a shared cache would leak one user's result to another.
//!   Per-user keying still collapses the common case (a dashboard re-invoking
//!   on reload for the same logged-in user).
//! - `hash(body)` — the invocation input.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use lru::LruCache;
use parking_lot::Mutex;
use uuid::Uuid;

/// Entry cap across all functions/users. Count-bounded for simplicity.
const MAX_ENTRIES: usize = 4096;

type Key = (Uuid, String, Uuid, u64);
/// value = (stored_at, ttl, body). Per-entry TTL so functions can differ.
type Cache = Mutex<LruCache<Key, (Instant, Duration, Arc<String>)>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(MAX_ENTRIES).expect("MAX_ENTRIES > 0"),
        ))
    })
}

fn key(build_id: Uuid, function: &str, user_id: Uuid, body: &[u8]) -> Key {
    let mut h = DefaultHasher::new();
    body.hash(&mut h);
    (build_id, function.to_string(), user_id, h.finish())
}

/// The cached response body if present and within its per-entry TTL; else
/// `None` (and a stale entry is evicted).
pub fn get(build_id: Uuid, function: &str, user_id: Uuid, body: &[u8]) -> Option<Arc<String>> {
    let mut c = cache().lock();
    let k = key(build_id, function, user_id, body);
    match c.get(&k) {
        Some((at, ttl, v)) if at.elapsed() < *ttl => Some(v.clone()),
        Some(_) => {
            c.pop(&k);
            None
        }
        None => None,
    }
}

/// Store a successful function result under its TTL.
pub fn put(
    build_id: Uuid,
    function: &str,
    user_id: Uuid,
    body: &[u8],
    value: String,
    ttl: Duration,
) {
    cache().lock().put(
        key(build_id, function, user_id, body),
        (Instant::now(), ttl, Arc::new(value)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_then_get_hits_within_ttl() {
        let b = Uuid::new_v4();
        let u = Uuid::new_v4();
        put(
            b,
            "f",
            u,
            b"{}",
            "RESULT".to_string(),
            Duration::from_secs(60),
        );
        assert_eq!(
            get(b, "f", u, b"{}").as_deref().map(String::as_str),
            Some("RESULT")
        );
    }

    #[test]
    fn misses_on_different_build_user_fn_or_body() {
        let b = Uuid::new_v4();
        let u = Uuid::new_v4();
        put(b, "f", u, b"{}", "R".to_string(), Duration::from_secs(60));
        assert!(
            get(Uuid::new_v4(), "f", u, b"{}").is_none(),
            "build isolation"
        );
        assert!(
            get(b, "f", Uuid::new_v4(), b"{}").is_none(),
            "user isolation"
        );
        assert!(get(b, "g", u, b"{}").is_none(), "function isolation");
        assert!(get(b, "f", u, b"{\"x\":1}").is_none(), "body isolation");
    }

    #[test]
    fn expires_after_ttl() {
        let b = Uuid::new_v4();
        let u = Uuid::new_v4();
        put(b, "f", u, b"{}", "R".to_string(), Duration::from_millis(0));
        std::thread::sleep(Duration::from_millis(5));
        assert!(get(b, "f", u, b"{}").is_none(), "expired entry must miss");
    }
}
