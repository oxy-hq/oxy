//! Shared TTL caches for the customer-apps hot paths.
//!
//! Both the bundle-serve handler (`customer_apps_serve`) and the
//! debug snapshot handler (`customer_apps_debug`) run the same
//! per-request resolution chain — authenticate, look up user, check
//! org membership, locate bundle dir — for every asset request and every
//! product fetch. A single Next.js page load is 30-100 asset requests +
//! N parallel product fetches; without caching, that's hundreds of DB
//! hits per click.
//!
//! These caches are small, in-process, TTL'd. Eviction is a sweep of
//! expired entries when a write would push past a soft cap — no LRU
//! machinery. The TTL is intentionally short (60s) so that membership
//! revocations propagate within a minute without requiring explicit
//! invalidation.

use std::collections::HashMap;
use std::hash::Hash;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

use oxy_auth::types::AuthenticatedUser;
use uuid::Uuid;

/// Lifetime of cache entries before they need re-fetching. Short enough
/// that org membership revocations propagate quickly; long enough to
/// absorb the asset-storm a Next.js page load triggers.
pub(super) const CACHE_TTL: Duration = Duration::from_secs(60);

/// Soft cap on cache size. A sweep of expired entries runs whenever a
/// write would push the cache past this threshold; if the sweep doesn't
/// reclaim anything (everything still fresh), the new entry still goes
/// in. Bounds the memory cost without forcing an LRU implementation.
pub(super) const CACHE_MAX_ENTRIES: usize = 4_096;

// ── Generic TTL map helpers ─────────────────────────────────────────────────

pub(super) fn get_fresh<K: Eq + Hash, V: Clone>(
    cache: &RwLock<HashMap<K, (V, Instant)>>,
    key: &K,
) -> Option<V> {
    let guard = cache.read().ok()?;
    let (value, inserted_at) = guard.get(key)?;
    if inserted_at.elapsed() > CACHE_TTL {
        return None;
    }
    Some(value.clone())
}

pub(super) fn insert_with_sweep<K: Eq + Hash, V>(
    cache: &RwLock<HashMap<K, (V, Instant)>>,
    key: K,
    value: V,
) {
    if let Ok(mut guard) = cache.write() {
        if guard.len() >= CACHE_MAX_ENTRIES {
            guard.retain(|_, (_, inserted_at)| inserted_at.elapsed() <= CACHE_TTL);
        }
        guard.insert(key, (value, Instant::now()));
    }
}

pub(super) fn remove_entry<K: Eq + Hash, V>(cache: &RwLock<HashMap<K, (V, Instant)>>, key: &K) {
    if let Ok(mut guard) = cache.write() {
        guard.remove(key);
    }
}

// ── User cache (email-keyed) ────────────────────────────────────────────────

fn user_cache() -> &'static RwLock<HashMap<String, (AuthenticatedUser, Instant)>> {
    static CACHE: OnceLock<RwLock<HashMap<String, (AuthenticatedUser, Instant)>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

pub(super) fn cached_user(email: &str) -> Option<AuthenticatedUser> {
    get_fresh(user_cache(), &email.to_string())
}

pub(super) fn set_cached_user(email: String, user: AuthenticatedUser) {
    insert_with_sweep(user_cache(), email, user);
}

// Org-membership caching now lives in `customer_apps_auth` alongside the
// combined (member | grant | global admin) access check, keyed on
// (user_id, app_id) — see that module for the rationale.

// ── Canonical bundle dir cache (app id × channel) ───────────────────────────
//
// Bundle dirs differ per channel for S3-source apps:
//   - draft:     $STATE/customer-apps/<uuid>/draft/
//   - published: $STATE/customer-apps/<uuid>/published/
// A staff member with the `oxy_preview_draft` cookie can fetch the draft
// channel; an unauthenticated customer (or a default staff request) gets
// published. Keying the cache on `uuid` alone leaks the first-resolved
// channel to all later requests until TTL expires — staff preview can
// contaminate customer views and vice versa.
//
// Local-source apps don't have channels, but we still need a cache key
// distinct from S3's; we use the literal `"local"` for that.
//
// Redeploys produce a new uuid in our model, but we still TTL the cache
// so a deploy mistake (deleted dir, then re-created) self-heals without
// a process restart.

/// Channel discriminator for the cache key. `&'static str` rather than
/// the heavier `Channel` enum so the key stays `Copy + Hash` cheap.
type ChannelKey = &'static str;

/// The two real channel names plus the `"local"` sentinel for local-
/// folder source apps. Callers pass one of these (or use the helper
/// methods on `Channel::as_cache_key()` once channel is in scope).
pub(super) const CACHE_CHANNEL_LOCAL: ChannelKey = "local";
pub(super) const CACHE_CHANNEL_DRAFT: ChannelKey = "draft";
pub(super) const CACHE_CHANNEL_PUBLISHED: ChannelKey = "published";

type CanonicalDirKey = (Uuid, ChannelKey);

fn canonical_dir_cache() -> &'static RwLock<HashMap<CanonicalDirKey, (PathBuf, Instant)>> {
    static CACHE: OnceLock<RwLock<HashMap<CanonicalDirKey, (PathBuf, Instant)>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

pub(super) fn cached_canonical_dir(id: Uuid, channel: ChannelKey) -> Option<PathBuf> {
    get_fresh(canonical_dir_cache(), &(id, channel))
}

pub(super) fn set_cached_canonical_dir(id: Uuid, channel: ChannelKey, path: PathBuf) {
    insert_with_sweep(canonical_dir_cache(), (id, channel), path);
}

/// Drop the cached canonical dir for an app on a specific channel.
/// Called when the symlink-escape check trips a 403 — without this, an
/// operator who fixes the bad symlink would wait up to `CACHE_TTL` for
/// the stale entry to expire on its own.
pub(super) fn invalidate_cached_canonical_dir(id: Uuid, channel: ChannelKey) {
    remove_entry(canonical_dir_cache(), &(id, channel));
}

/// Drop the cached canonical dir entries for ALL channels of an app.
/// Used by `publish_app` / `unpublish_app` so a freshly-published
/// bundle starts serving immediately rather than waiting for the per-
/// channel TTL to expire.
pub(super) fn invalidate_cached_canonical_dir_all_channels(id: Uuid) {
    if let Ok(mut guard) = canonical_dir_cache().write() {
        guard.retain(|(uuid, _), _| *uuid != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_dir_cache_invalidates_on_demand() {
        let id = Uuid::new_v4();
        set_cached_canonical_dir(id, CACHE_CHANNEL_LOCAL, PathBuf::from("/tmp/x"));
        assert!(cached_canonical_dir(id, CACHE_CHANNEL_LOCAL).is_some());
        invalidate_cached_canonical_dir(id, CACHE_CHANNEL_LOCAL);
        assert!(cached_canonical_dir(id, CACHE_CHANNEL_LOCAL).is_none());
    }

    #[test]
    fn canonical_dir_cache_keys_distinct_channels_separately() {
        // The reason this cache key was widened in the first place: a
        // staff request resolving draft must not be served back to a
        // customer asking for published, and vice versa.
        let id = Uuid::new_v4();
        set_cached_canonical_dir(id, CACHE_CHANNEL_DRAFT, PathBuf::from("/tmp/draft"));
        set_cached_canonical_dir(id, CACHE_CHANNEL_PUBLISHED, PathBuf::from("/tmp/pub"));
        assert_eq!(
            cached_canonical_dir(id, CACHE_CHANNEL_DRAFT),
            Some(PathBuf::from("/tmp/draft")),
        );
        assert_eq!(
            cached_canonical_dir(id, CACHE_CHANNEL_PUBLISHED),
            Some(PathBuf::from("/tmp/pub")),
        );
    }

    #[test]
    fn invalidate_all_channels_clears_both_draft_and_published() {
        let id = Uuid::new_v4();
        let other = Uuid::new_v4();
        set_cached_canonical_dir(id, CACHE_CHANNEL_DRAFT, PathBuf::from("/tmp/a"));
        set_cached_canonical_dir(id, CACHE_CHANNEL_PUBLISHED, PathBuf::from("/tmp/b"));
        set_cached_canonical_dir(other, CACHE_CHANNEL_DRAFT, PathBuf::from("/tmp/c"));
        invalidate_cached_canonical_dir_all_channels(id);
        assert!(cached_canonical_dir(id, CACHE_CHANNEL_DRAFT).is_none());
        assert!(cached_canonical_dir(id, CACHE_CHANNEL_PUBLISHED).is_none());
        // Other app's entries left intact.
        assert!(cached_canonical_dir(other, CACHE_CHANNEL_DRAFT).is_some());
    }
}
