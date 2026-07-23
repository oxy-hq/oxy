//! In-memory LRU over custom-app bundle objects.
//!
//! The serve path resolves `(app_id, build_id, rel_path)` to bytes; this
//! cache absorbs hot reads so S3 is hit only on a miss. Keyed by
//! `"<app_id>/<build_id>/<rel_path>"` — because `build_id` is part of the
//! key, a promote/rollback (which repoints to a different build) serves
//! fresh bytes with no explicit invalidation.

use std::num::NonZeroUsize;
use std::sync::{Arc, OnceLock};

use lru::LruCache;
use parking_lot::Mutex;
use uuid::Uuid;

use super::custom_apps_build_store::{self, BuildStoreError};

/// Entry cap. Bundles are small (a handful of files each); a few thousand
/// entries covers many concurrently-hot apps. Count-bounded rather than
/// byte-bounded for simplicity — revisit if a single app ships huge assets.
const MAX_ENTRIES: usize = 4096;

type Cache = Mutex<LruCache<String, Arc<Vec<u8>>>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(MAX_ENTRIES).expect("MAX_ENTRIES > 0"),
        ))
    })
}

fn key(app_id: Uuid, build_id: &str, rel_path: &str) -> String {
    format!("{app_id}/{build_id}/{}", rel_path.trim_start_matches('/'))
}

/// Return the object bytes, fetching from S3 on a cache miss and caching
/// the result. `Ok(None)` when the object does not exist in the build.
pub async fn get_or_fetch(
    app_id: Uuid,
    build_id: &str,
    rel_path: &str,
) -> Result<Option<Arc<Vec<u8>>>, BuildStoreError> {
    let k = key(app_id, build_id, rel_path);
    if let Some(hit) = cache().lock().get(&k).cloned() {
        return Ok(Some(hit));
    }
    match custom_apps_build_store::get_object(app_id, build_id, rel_path).await? {
        Some(bytes) => {
            let arc = Arc::new(bytes);
            cache().lock().put(k, arc.clone());
            Ok(Some(arc))
        }
        None => Ok(None),
    }
}

/// Warm the cache directly (used at publish time so the first viewer of a
/// freshly-published `index.html` doesn't pay the cold S3 round trip).
pub fn seed(app_id: Uuid, build_id: &str, rel_path: &str, bytes: Arc<Vec<u8>>) {
    cache().lock().put(key(app_id, build_id, rel_path), bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn seeded_entry_served_without_s3() {
        // Seeding first proves the cache short-circuits the store entirely:
        // the lookup returns the seeded bytes without ever calling
        // get_object (no S3 round trip, no filesystem read).
        let app = Uuid::new_v4();
        let bytes = Arc::new(b"<html>seeded".to_vec());
        seed(app, "bx", "index.html", bytes.clone());
        let got = get_or_fetch(app, "bx", "index.html")
            .await
            .expect("cache hit must not touch S3");
        assert_eq!(
            got.as_deref().map(|v| v.as_slice()),
            Some(&b"<html>seeded"[..])
        );
    }
}
