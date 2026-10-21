//! Build-once-per-key for one invocation: the warehouse connectors
//! `ProjectFunctionHost::connect` hands out (`host.rs`).
//!
//! One `tokio::sync::OnceCell` per key, behind a map lock held only to find
//! or insert the cell: a key's first caller builds while later callers of
//! the same key wait on its cell, and a call for another key never waits —
//! a slow or hanging handshake on database A does not stall database B. A
//! failed build leaves its cell empty, so the next call retries rather than
//! caching the failure.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::OnceCell;

/// A per-key cache of values built at most once each per instance.
pub(super) struct BuildOnce<T> {
    cells: Mutex<HashMap<String, Arc<OnceCell<T>>>>,
}

impl<T> Default for BuildOnce<T> {
    fn default() -> Self {
        Self {
            cells: Mutex::new(HashMap::new()),
        }
    }
}

impl<T: Clone> BuildOnce<T> {
    /// The value for `key`, built by `build` on the first call that reaches
    /// it (or the first after a failed build). Concurrent calls for one key
    /// share one build; calls for other keys run independently.
    pub(super) async fn get_or_build<F, Fut>(&self, key: &str, build: F) -> Result<T, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, String>>,
    {
        let cell = self
            .cells
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(key.to_string())
            .or_default()
            .clone();
        cell.get_or_try_init(build).await.cloned()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[tokio::test]
    async fn a_key_is_built_once_and_reused() {
        let cache = BuildOnce::<u32>::default();
        let builds = AtomicUsize::new(0);
        let build = || async {
            builds.fetch_add(1, Ordering::SeqCst);
            Ok(7)
        };
        assert_eq!(cache.get_or_build("a", build).await, Ok(7));
        assert_eq!(cache.get_or_build("a", build).await, Ok(7));
        assert_eq!(builds.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_failed_build_is_not_cached() {
        let cache = BuildOnce::<u32>::default();
        let builds = AtomicUsize::new(0);
        let build = || async {
            match builds.fetch_add(1, Ordering::SeqCst) {
                0 => Err("handshake failed".to_string()),
                _ => Ok(1),
            }
        };
        assert_eq!(
            cache.get_or_build("a", build).await,
            Err("handshake failed".to_string())
        );
        assert_eq!(cache.get_or_build("a", build).await, Ok(1));
        assert_eq!(builds.load(Ordering::SeqCst), 2);
    }

    /// Key A's build waits until key B has been served: a map-wide lock held
    /// across the build would deadlock this, a per-key cell does not.
    #[tokio::test]
    async fn a_slow_key_does_not_stall_another() {
        let cache = BuildOnce::<&'static str>::default();
        let b_served = tokio::sync::Notify::new();
        let slow_a = cache.get_or_build("a", || async {
            b_served.notified().await;
            Ok("a")
        });
        let quick_b = async {
            let b = cache.get_or_build("b", || async { Ok("b") }).await;
            b_served.notify_one();
            b
        };
        let (a, b) = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            futures::future::join(slow_a, quick_b),
        )
        .await
        .expect("b must be served while a is still building");
        assert_eq!((a, b), (Ok("a"), Ok("b")));
    }
}
