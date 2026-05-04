//! In-memory feature-flag cache.
//!
//! Single-instance only. The cache is preloaded once at server startup
//! and updated in-place on every admin PATCH. Reads are pure HashMap
//! lookups — no DB hit at the read site, ever.
//!
//! When Oxy goes horizontal, swap this for read-through with TTL or layer
//! Postgres LISTEN/NOTIFY on top of the same `is_enabled()` API.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

use sea_orm::{DatabaseConnection, DbErr};

use super::registry;
use super::store;

static CACHE: OnceLock<RwLock<HashMap<&'static str, bool>>> = OnceLock::new();
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Fetches all DB rows, builds the initial map (one entry per registry
/// flag), installs it into the cache, and marks the cache as initialized.
/// Stale DB rows whose key is not in the registry are logged and skipped.
pub async fn init(db: &DatabaseConnection) -> Result<(), DbErr> {
    let rows = store::fetch_all(db).await?;
    let mut by_key: HashMap<&str, bool> = HashMap::new();
    for row in &rows {
        if registry::get(&row.key).is_some() {
            by_key.insert(row.key.as_str(), row.enabled);
        } else {
            tracing::warn!(key = %row.key, "stale feature flag in DB, ignoring");
        }
    }

    let mut map: HashMap<&'static str, bool> = HashMap::new();
    for flag in registry::FLAGS {
        let value = by_key
            .get(flag.key)
            .copied()
            .unwrap_or(flag.default_enabled);
        map.insert(flag.key, value);
    }

    CACHE
        .set(RwLock::new(map))
        .map_err(|_| DbErr::Custom("feature_flags cache already initialized".into()))?;
    INITIALIZED.store(true, Ordering::Release);
    Ok(())
}

/// Returns whether `key` is enabled. Synchronous — pure HashMap lookup
/// after init. If the cache is uninitialized, returns the registry default
/// for `key` and emits a `tracing::error!`. Unknown keys return `false`.
pub fn is_enabled(key: &'static str) -> bool {
    if !INITIALIZED.load(Ordering::Acquire) {
        tracing::error!(
            key,
            "feature_flags::is_enabled called before cache init; using registry default"
        );
        return registry::default_for(key);
    }
    let Some(cache) = CACHE.get() else {
        return registry::default_for(key);
    };
    let guard = match cache.read() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    match guard.get(key).copied() {
        Some(v) => v,
        None => {
            tracing::warn!(key, "feature_flags::is_enabled called with unknown key");
            false
        }
    }
}

/// Overwrites the cache entry for `key` with `enabled`. Called by the
/// PATCH handler after the DB write commits.
pub fn set(key: &'static str, enabled: bool) {
    let Some(cache) = CACHE.get() else {
        tracing::error!(key, "feature_flags::set called before cache init");
        return;
    };
    let mut guard = match cache.write() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.insert(key, enabled);
}

#[cfg(test)]
pub fn init_for_tests(values: HashMap<&'static str, bool>) {
    if CACHE.get().is_none() {
        let _ = CACHE.set(RwLock::new(HashMap::new()));
    }
    let cache = CACHE.get().expect("cache slot installed above");
    let mut guard = match cache.write() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    *guard = values;
    INITIALIZED.store(true, Ordering::Release);
}

#[cfg(test)]
pub fn override_for_tests(key: &'static str, enabled: bool) {
    if CACHE.get().is_none() {
        let _ = CACHE.set(RwLock::new(HashMap::new()));
    }
    INITIALIZED.store(true, Ordering::Release);
    set(key, enabled);
}
