//! The producers' gate: is anyone tailing this `(app, environment)`?
//!
//! Asked by every function invocation (`producer`), so it must cost nothing
//! when nobody tails anything. This process keeps the watched set from
//! [`watchers::watched_pairs`] and reads the table again at most once every
//! [`REFRESH_EVERY`]; between reads the answer is a set lookup. A refresh that
//! fails keeps the last set and waits out the interval before trying again,
//! so a database blip costs one warning a second, not a query per call.
//!
//! The stream's half of the contract is [`SETTLE`]: it waits that long after
//! writing its lease before it says `open`, so by then every process's next
//! answer includes the new pair.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

use sea_orm::ConnectionTrait;
use uuid::Uuid;

use super::watchers;

/// The longest a process goes between reads of the watcher table while it
/// is producing.
pub const REFRESH_EVERY: Duration = Duration::from_secs(1);

/// How long a new stream waits after registering before it reports `open`:
/// one refresh interval and a margin, so a process whose last read predates
/// the lease reads again before any call it starts after `open`.
pub const SETTLE: Duration = Duration::from_millis(1_100);

// Waiting longer than one refresh is what makes a call started after `open`
// see the new pair.
const _: () = assert!(SETTLE.as_millis() > REFRESH_EVERY.as_millis());

type Pairs = Arc<HashSet<(Uuid, String)>>;

struct Cache {
    read_at: Option<Instant>,
    pairs: Pairs,
}

fn cache() -> &'static RwLock<Cache> {
    static CACHE: OnceLock<RwLock<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| {
        RwLock::new(Cache {
            read_at: None,
            pairs: Arc::default(),
        })
    })
}

/// One refresh at a time per process; the rest answer from the last set.
static REFRESHING: AtomicBool = AtomicBool::new(false);

fn stale(now: Instant) -> bool {
    let cache = cache().read().unwrap_or_else(|p| p.into_inner());
    cache
        .read_at
        .is_none_or(|at| now.duration_since(at) >= REFRESH_EVERY)
}

fn store(pairs: Option<HashSet<(Uuid, String)>>, now: Instant) {
    let mut cache = cache().write().unwrap_or_else(|p| p.into_inner());
    cache.read_at = Some(now);
    if let Some(pairs) = pairs {
        cache.pairs = Arc::new(pairs);
    }
}

/// The one refresh in flight. Released on drop, so a refresh whose caller
/// was cancelled mid-read does not stop this process refreshing for good.
struct Refreshing;

impl Refreshing {
    fn claim() -> Option<Self> {
        (!REFRESHING.swap(true, Ordering::AcqRel)).then_some(Self)
    }
}

impl Drop for Refreshing {
    fn drop(&mut self) {
        REFRESHING.store(false, Ordering::Release);
    }
}

/// Read the watcher table again when this process's copy is older than
/// [`REFRESH_EVERY`] and no other task is reading it already. The copy is
/// stamped with the time the read began, so it is never thought fresher
/// than it is.
async fn refresh_if_stale(db: &impl ConnectionTrait) {
    let now = Instant::now();
    if !stale(now) {
        return;
    }
    let Some(_claim) = Refreshing::claim() else {
        return;
    };
    match watchers::watched_pairs(db).await {
        Ok(pairs) => store(Some(pairs), now),
        Err(e) => {
            tracing::warn!(error = %e, "tail: reading the watcher table failed; keeping the last set");
            store(None, now);
        }
    }
}

/// Whether `environment` of `app_id` is being tailed, refreshing this
/// process's watched set first when it is stale.
pub async fn is_watched(db: &impl ConnectionTrait, app_id: Uuid, environment: &str) -> bool {
    refresh_if_stale(db).await;
    is_watched_now(app_id, environment)
}

/// [`is_watched`] against the set this process already holds, without a
/// read: for the one moment a producer cannot await (an invocation's end).
pub fn is_watched_now(app_id: Uuid, environment: &str) -> bool {
    let pairs = {
        let cache = cache().read().unwrap_or_else(|p| p.into_inner());
        Arc::clone(&cache.pairs)
    };
    pairs.contains(&(app_id, environment.to_string()))
}

/// Test seam: set this process's watched set as if just read.
#[cfg(test)]
pub(super) fn set_for_test(pairs: impl IntoIterator<Item = (Uuid, String)>) {
    store(Some(pairs.into_iter().collect()), Instant::now());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The answer is per `(app, environment)`: another environment of the
    /// same app, or the same name of another app, is not watched.
    #[test]
    fn only_the_watched_pair_is_watched() {
        let (app, other) = (Uuid::new_v4(), Uuid::new_v4());
        set_for_test([(app, "dev-a1".to_string())]);
        assert!(is_watched_now(app, "dev-a1"));
        assert!(!is_watched_now(app, "dev-b2"));
        assert!(!is_watched_now(app, "production"));
        assert!(!is_watched_now(other, "dev-a1"));
    }

    /// A set read just now is fresh; one older than the interval is stale.
    #[test]
    fn a_set_is_stale_after_one_interval() {
        set_for_test([]);
        let now = Instant::now();
        assert!(!stale(now));
        assert!(stale(now + REFRESH_EVERY));
    }
}
