//! In-memory LRU over custom-app bundle objects.
//!
//! The serve path resolves `(app_id, build_id, rel_path)` to bytes; this
//! cache absorbs hot reads so S3 is hit only on a miss. Keyed by
//! `"<app_id>/<build_id>/<rel_path>"` — because `build_id` is part of the
//! key, a promote/rollback (which repoints to a different build) serves
//! fresh bytes with no explicit invalidation.
//!
//! ## Absences are cached too
//!
//! A *known-absent* object is remembered rather than re-fetched, in a
//! second LRU held apart from the bytes (see [`byte_cache`] for why the two
//! must not share a map). A build's file set is fixed once
//! `put_build` returns — the prefix is written once and never appended to —
//! so "not in this build" is a permanent fact for that `build_id`, and
//! caching it is safe.
//!
//! That invariant is **enforced, not assumed**. `put_build` never wipes a
//! prefix before writing, so a second publish under a reused `build_id` would
//! merge into the first — and a file the rebuild added would read as
//! permanently absent here, which for a `.js` request means the SPA
//! `index.html` fallback at 200 rather than the asset. Two things keep that
//! from happening: `custom_apps_publish` rejects a publish whose
//! `(app_id, build_id)` already has an `app_builds` row (409), and the CLI's
//! default id is unique per CI *run*, not per commit, so a workflow re-run
//! doesn't collide in the first place (`sdk/cli/src/publish/provenance.ts::ciBuildId`).
//! If either is ever relaxed, this section is the thing that breaks.
//!
//! Two hot paths depend on this, and both re-fetched on *every* request
//! before it existed:
//!   - **SPA fallback.** A client-side route (`/orders/42`) misses the
//!     object store, then falls back to `index.html`. Without negative
//!     caching that's one doomed store round-trip per navigation, forever.
//!   - **Pre-compressed variants.** The serve path probes `<path>.br`
//!     (see `custom_apps_precompress`). Builds published before
//!     pre-compression existed have no `.br` objects at all, so every
//!     asset request on an old build would pay a doomed probe.
//!
//! Errors are **not** cached — only a definitive present/absent answer is,
//! so a transient S3 failure retries on the next request.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, OnceLock};

use bytes::Bytes;

use lru::LruCache;
use parking_lot::Mutex;
use uuid::Uuid;

use crate::build_store::{self, BuildStoreError};

/// Entry cap for cached **bytes**. Retained alongside the byte budget below;
/// whichever binds first wins.
///
/// It is no longer the *interesting* bound — see [`MAX_BYTE_CACHE_BYTES`] —
/// but it stays because the two answer different questions. This one bounds
/// per-entry bookkeeping (a map of 8192 tiny assets costs the same map
/// overhead whatever they weigh), and it is what keeps the LRU's own
/// allocation predictable.
///
/// A build contributes roughly its file count, plus — for an asset requested
/// by both a brotli and a non-brotli client — one entry per representation.
/// The `.br`-first probe in `custom_apps_serve::sources` keeps the common
/// case to a single entry per asset, since a brotli hit never fetches the
/// identity object.
const MAX_BYTE_ENTRIES: usize = 8192;

/// Resident-byte budget for the byte cache. Override with
/// [`MAX_BYTE_CACHE_BYTES_ENV`]; `0` disables the byte bound and leaves only
/// the entry cap.
///
/// **Why this exists.** The entry cap alone bounded the map backwards relative
/// to its value: `MAX_ABSENT_REL_LEN` length-bounds the map whose entries are
/// ≤ ~550 bytes, while the map holding whole assets was bounded only by a
/// *count*. 8192 slots × a multi-MiB chunk is GiBs resident on a replica
/// hosting many apps, and **every publisher sizes their own assets** — so the
/// ceiling was set by tenants rather than by us, with the pod's 2 GiB cgroup
/// limit as the only real backstop. That limit kills the process, which serves
/// every other app.
///
/// 256 MiB against a serve process whose 14-day peak was 888 MiB on a 2 GiB
/// limit: large enough that a busy app's whole critical path stays hot, small
/// enough that the cache cannot be the reason a replica reaches the ceiling.
/// It is a budget, not a measurement — `oxy_custom_app_bundle_cache_bytes`
/// reports what is actually resident, and the eviction counter says whether
/// the budget binds.
const MAX_BYTE_CACHE_BYTES: usize = 256 * 1024 * 1024;

/// Override for [`MAX_BYTE_CACHE_BYTES`], in megabytes. `0` disables.
pub const MAX_BYTE_CACHE_BYTES_ENV: &str = "OXY_BUNDLE_CACHE_MAX_MB";

/// The largest single object worth caching, as a fraction of the budget.
///
/// An asset bigger than this is served straight through. Admitting it would
/// evict a large share of everything else to hold one object that is itself
/// then first in line to go — paying the eviction cost twice for no hit-rate.
/// An eighth means at least eight large assets coexist before the budget
/// binds, which is the point where an LRU still behaves like a cache rather
/// than a two-entry buffer.
const OVERSIZE_DIVISOR: usize = 8;

/// Entry cap for **known-absent** keys. Larger than the byte cap because an
/// entry is a bare string rather than a payload, and because the key space
/// here is genuinely unbounded: `rel` comes from the request URL, so every
/// distinct client-side route a visitor hits records one.
const MAX_ABSENT_ENTRIES: usize = 16384;

/// Longest `rel` worth remembering as absent.
///
/// The cap above counts entries, but the key embeds `rel` verbatim and
/// `is_safe_rel` constrains only its *shape*, never its length — so a count
/// cap alone leaves the resident bytes of this map under request control.
/// A real bundle path is far under this, so declining to record longer ones
/// costs nothing on any genuine request. The pathological URL re-pays a store
/// round-trip on **every** request rather than once per process — not a
/// one-off — which is the deliberate trade: the alternative is resident bytes
/// under request control. Hashing the tail into the key would keep both
/// properties if that ever stops being the right call.
///
/// The bound is on `rel_path`, while the stored key is
/// `"<uuid>/<build_id>/<rel>"` and legacy `build_id`s have no length cap
/// (`MAX_BUILD_ID_LEN` gates new publishes only), so the true per-entry
/// ceiling is `512 + |build_id| + 37`.
const MAX_ABSENT_REL_LEN: usize = 512;

/// Cached object bytes.
///
/// `Bytes`, not `Arc<Vec<u8>>`: the serve path hands these straight to
/// `Body::from`, which takes `Bytes` by value and refcounts it. Holding
/// `Arc<Vec<u8>>` forced a `to_vec()` at that boundary — a full alloc and
/// memcpy of the asset on every warm-cache hit, which is exactly the
/// per-request cost this module exists to remove.
/// An LRU that evicts on **resident bytes** as well as entry count.
///
/// The running total lives beside the map rather than being recomputed,
/// because `Bytes::len()` is O(1) but summing 8192 of them per insert is not.
/// Both fields are private and every mutation goes through the three methods
/// below, so `resident` cannot drift from the sum of the entries — the one
/// invariant that matters here, and the one a bare `usize` next to a public
/// map would eventually lose.
///
/// The subtle case is **replacement**: `LruCache::put` returns the displaced
/// value, and forgetting to subtract its length leaks budget on every
/// overwrite until the cache believes it is full while holding almost
/// nothing. A test pins it.
struct ByteBudgetCache {
    lru: LruCache<String, Bytes>,
    resident: usize,
    /// Held per-instance rather than read from [`byte_budget`] at each `put`,
    /// so a test can build a small cache and observe eviction without
    /// allocating the real 256 MiB budget.
    budget: usize,
}

impl ByteBudgetCache {
    fn new() -> Self {
        Self::with_budget(byte_budget())
    }

    fn with_budget(budget: usize) -> Self {
        Self::with_caps(MAX_BYTE_ENTRIES, budget)
    }

    /// Both caps injectable, so a test can make the **entry** cap bind without
    /// inserting 8192 objects. That matters more than it looks: the entry cap
    /// is the one that binds first in production whenever the mean cached
    /// object is under `budget / MAX_BYTE_ENTRIES` (32 KiB at the defaults),
    /// which is most of a Vite build — and it is the path where the byte
    /// accounting was wrong.
    fn with_caps(entries: usize, budget: usize) -> Self {
        Self {
            lru: LruCache::new(NonZeroUsize::new(entries).expect("entry cap > 0")),
            resident: 0,
            budget,
        }
    }

    fn get(&mut self, k: &str) -> Option<Bytes> {
        self.lru.get(k).cloned()
    }

    /// Insert, evicting least-recently-used entries until the budget holds.
    ///
    /// Returns `false` when the object was too large to admit (see
    /// [`OVERSIZE_DIVISOR`]) — the caller still serves it, it just is not
    /// remembered.
    fn put(&mut self, k: String, v: Bytes) -> bool {
        let budget = self.budget;
        if budget > 0 && v.len() > budget / OVERSIZE_DIVISOR {
            return false;
        }
        let incoming = v.len();
        // `contains` takes `&self` and cannot promote, so probing first leaves
        // LRU order untouched. It is what distinguishes the two things `push`
        // reports through one return value.
        let replacing = self.lru.contains(&k);
        let mut evicted = 0u64;

        // `push`, NOT `put`. `put` returns only a *replaced* value: at capacity
        // it evicts the least-recently-used entry internally and returns
        // `None`, because the victim is a different key than the one inserted
        // and `Option<V>` cannot name it. Its bytes were therefore never
        // subtracted, so `resident` drifted upward on every insert past the
        // entry cap — over-reporting the gauge, then evicting live entries to
        // pay down phantom bytes, converging on evict-per-insert. `push`
        // returns `Option<(K, V)>` and reports both cases.
        //
        // Replacement is handled before the budget check, so overwriting a hot
        // entry with a slightly larger one does not evict others for nothing.
        if let Some((_, old)) = self.lru.push(k, v) {
            self.resident = self.resident.saturating_sub(old.len());
            if !replacing {
                // A capacity eviction rather than an overwrite. Counting it is
                // what keeps the counter from measuring byte-budget pressure
                // alone — otherwise "zero evictions" reads as "the budget is
                // generous" on a cache evicting steadily on entries.
                evicted += 1;
            }
        }
        self.resident += incoming;

        if budget > 0 {
            while self.resident > budget {
                match self.lru.pop_lru() {
                    Some((_, victim)) => {
                        self.resident = self.resident.saturating_sub(victim.len());
                        evicted += 1;
                    }
                    // Reachable only if `resident` has drifted from the map —
                    // which is the bug above. Kept so that a future drift is a
                    // wrong number rather than an infinite loop under the
                    // process-global lock on the request path.
                    None => {
                        self.resident = 0;
                        break;
                    }
                }
            }
        }

        // Recorded once, not per victim. A large insert must free its own size,
        // so a 32 MiB object against 4 KiB chunks is thousands of pops — and
        // this loop runs under the mutex every custom-app asset request
        // contends on. One `add(n)` keeps the metrics cost O(1) per insert
        // instead of O(victims) on a path `oxy-customer-apps-perf` governs.
        if evicted > 0 {
            oxy_telemetry::metrics::record::bundle_cache_evictions(evicted);
        }
        oxy_telemetry::metrics::sources::set_bundle_cache_bytes(self.resident as u64);
        true
    }

    /// Only the tests reach this — `seed` pops the *absent* cache, which is a
    /// plain `LruCache`. Kept because the byte accounting has to be exercised
    /// on removal too, and gated so it is not a dead-code warning in a build
    /// that does not compile tests.
    #[cfg(test)]
    fn pop(&mut self, k: &str) {
        if let Some(v) = self.lru.pop(k) {
            self.resident = self.resident.saturating_sub(v.len());
            oxy_telemetry::metrics::sources::set_bundle_cache_bytes(self.resident as u64);
        }
    }
}

/// The resident-byte budget in force, resolved once.
///
/// **Call [`resolve_budget`] at boot rather than letting this fall out of the
/// first request** — see that function for why.
fn byte_budget() -> usize {
    static VALUE: OnceLock<usize> = OnceLock::new();
    *VALUE.get_or_init(|| {
        let bytes = match std::env::var(MAX_BYTE_CACHE_BYTES_ENV)
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
        {
            Some(0) => 0,
            // `saturating_mul`: the parse accepts any `usize`, so a fat-fingered
            // `OXY_BUNDLE_CACHE_MAX_MB` overflows the multiply — a panic in a
            // debug build (which is what this repo builds locally and in CI)
            // and a silent wrap in release, both inside a `OnceLock` init on
            // the first request that touches the cache.
            Some(mb) => mb.saturating_mul(1024 * 1024),
            None => MAX_BYTE_CACHE_BYTES,
        };
        oxy_telemetry::metrics::sources::set_bundle_cache_limit(bytes as u64);
        bytes
    })
}

/// Resolve the budget and publish `oxy_custom_app_bundle_cache_limit_bytes`.
///
/// Called from serve startup. Left lazy, the budget is resolved by the first
/// custom-app asset request on the replica, so until then the gauge reads `0` —
/// which is also the documented value for "the byte bound is disabled". Over
/// the whole boot-to-first-bundle-request window the two are indistinguishable,
/// and the saturation ratio this gauge is the denominator of divides by zero.
///
/// That is the same defect the admission-limit gauge already had and already
/// fixed; `sources` states the rule in bold — publish at boot, not lazily —
/// and this reintroduced it for a sibling gauge. Unlike the admission limits
/// this is **not** behind `custom-app-functions`: the bundle cache serves
/// static assets and exists whether or not the V8 runtime is compiled in.
pub fn resolve_budget() -> usize {
    byte_budget()
}

type ByteCache = Mutex<ByteBudgetCache>;
/// Keys known not to exist in their build. Value-less: presence *is* the fact.
type AbsentCache = Mutex<LruCache<String, ()>>;

/// Bytes and absences live in **separate** LRUs rather than one map of
/// `Option<_>`.
///
/// Sharing one map lets negatives evict positives, and the two are not
/// remotely equal in value: an absence costs one store round-trip to
/// rediscover, while a positive costs a round-trip *and* the bytes. Because
/// absent keys are attacker-shaped — unbounded, URL-derived, and cheap to
/// generate — sustained traffic over many client-side routes on one app
/// (a crawler, a deep-linked list view) would otherwise evict *another*
/// app's hot asset bytes from this process-global cache. Splitting them
/// makes that structurally impossible instead of a sizing accident, and
/// lets each cap suit what it holds.
fn byte_cache() -> &'static ByteCache {
    static CACHE: OnceLock<ByteCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ByteBudgetCache::new()))
}

fn absent_cache() -> &'static AbsentCache {
    static CACHE: OnceLock<AbsentCache> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(MAX_ABSENT_ENTRIES).expect("MAX_ABSENT_ENTRIES > 0"),
        ))
    })
}

fn key(app_id: Uuid, build_id: &str, rel_path: &str) -> String {
    format!("{app_id}/{build_id}/{}", rel_path.trim_start_matches('/'))
}

/// Return the object bytes, fetching from the build store on a cache miss
/// and caching the outcome — including a definitive absence. `Ok(None)`
/// when the object does not exist in the build.
pub async fn get_or_fetch(
    app_id: Uuid,
    build_id: &str,
    rel_path: &str,
) -> Result<Option<Bytes>, BuildStoreError> {
    get_or_fetch_with(app_id, build_id, rel_path, || {
        build_store::get_object(app_id, build_id, rel_path)
    })
    .await
}

/// What a leader's fetch came to, as every follower of that fetch reads it.
#[derive(Clone)]
enum FlightOutcome {
    /// The store answered — bytes, a definitive absence, or an error — and
    /// that answer is every follower's answer too. An error is shared rather
    /// than retried per follower: under a store outage N waiters would
    /// otherwise wake one at a time and each take the same failed round
    /// trip, which is worse than the parallel misses this replaces.
    Answered(Result<Option<Bytes>, BuildStoreError>),
    /// The leader went away before the store answered — its request was
    /// cancelled. Followers start over, and one of them leads.
    Abandoned,
}

/// One miss in flight. The leader publishes its outcome here and every
/// follower reads it, so a cold asset asked for by a whole page load's worth
/// of parallel requests costs the store ONE round trip.
///
/// Before this, every concurrent miss on the same key went to the store:
/// the first viewer of a fresh publish — or of a replica that just came up —
/// is 30-100 requests arriving together, each missing, each paying the S3
/// round trip, each writing the same bytes into the cache.
struct Flight {
    outcome: tokio::sync::watch::Sender<Option<FlightOutcome>>,
}

/// The misses in flight right now, one per key. An entry lives exactly as
/// long as its leader's [`FlightLease`].
type InFlight = Mutex<HashMap<String, Arc<Flight>>>;

fn in_flight() -> &'static InFlight {
    static MAP: OnceLock<InFlight> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The leader's hold on its flight.
///
/// Dropping it — on return, on error, or on **cancellation** when the
/// request's future is dropped mid-fetch (a client that disconnected) —
/// removes the entry and, if no outcome was published, tells the followers
/// to start over. Without the RAII a cancelled leader left its entry behind
/// forever, and every later request for that key waited on a dead gate.
struct FlightLease {
    key: String,
    flight: Arc<Flight>,
    published: bool,
}

impl FlightLease {
    /// Hand the outcome to every follower, then release the key.
    fn publish(mut self, outcome: Result<Option<Bytes>, BuildStoreError>) {
        self.published = true;
        let _ = self
            .flight
            .outcome
            .send(Some(FlightOutcome::Answered(outcome)));
    }
}

impl Drop for FlightLease {
    fn drop(&mut self) {
        // Only OUR entry: a newcomer that arrived after an earlier release
        // may have opened a fresh flight under the same key, and that one
        // belongs to its own leader.
        let mut map = in_flight().lock();
        if map
            .get(&self.key)
            .is_some_and(|current| Arc::ptr_eq(current, &self.flight))
        {
            map.remove(&self.key);
        }
        drop(map);
        if !self.published {
            let _ = self.flight.outcome.send(Some(FlightOutcome::Abandoned));
        }
    }
}

enum Role {
    Lead(FlightLease),
    Follow(Arc<Flight>),
}

/// Join the flight already under way for `k`, or open one and lead it.
fn join_or_lead(k: &str) -> Role {
    let mut map = in_flight().lock();
    if let Some(flight) = map.get(k) {
        return Role::Follow(flight.clone());
    }
    let (outcome, _) = tokio::sync::watch::channel(None);
    let flight = Arc::new(Flight { outcome });
    map.insert(k.to_string(), flight.clone());
    Role::Lead(FlightLease {
        key: k.to_string(),
        flight,
        published: false,
    })
}

/// Wait for the leader's word. A sender that vanishes without one is read
/// as abandoned — unreachable while this holds the `Arc`, but the honest
/// fallback.
async fn follow(flight: &Flight) -> FlightOutcome {
    let mut rx = flight.outcome.subscribe();
    loop {
        if let Some(outcome) = rx.borrow_and_update().clone() {
            return outcome;
        }
        if rx.changed().await.is_err() {
            return FlightOutcome::Abandoned;
        }
    }
}

/// [`get_or_fetch`] over any store, so the single-flight can be proven
/// against a fake one. `fetch` runs at most once per key across every
/// concurrent caller: followers take the leader's answer, error included,
/// and the first request after the flight is released leads a fresh one.
async fn get_or_fetch_with<F, Fut>(
    app_id: Uuid,
    build_id: &str,
    rel_path: &str,
    fetch: F,
) -> Result<Option<Bytes>, BuildStoreError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Option<Bytes>, BuildStoreError>>,
{
    let k = key(app_id, build_id, rel_path);
    // Too long to ever be remembered as absent (`MAX_ABSENT_REL_LEN`), and
    // request-controlled: the in-flight map must not grow with a scanner's
    // URLs, so these are fetched ungated, as before.
    if rel_path.len() > MAX_ABSENT_REL_LEN {
        let fetched = fetch().await;
        if let Ok(outcome) = &fetched {
            remember(k, app_id, build_id, rel_path, outcome);
        }
        return fetched;
    }
    let mut fetch = Some(fetch);
    loop {
        if let Some(known) = lookup(&k) {
            return Ok(known);
        }
        match join_or_lead(&k) {
            Role::Lead(lease) => {
                // A caller leads at most once: a leader returns, so the
                // closure is still here for a follower that was abandoned.
                let fetch = fetch.take().expect("a caller leads at most once");
                let fetched = fetch().await;
                if let Ok(outcome) = &fetched {
                    remember(k.clone(), app_id, build_id, rel_path, outcome);
                }
                lease.publish(fetched.clone());
                return fetched;
            }
            Role::Follow(flight) => match follow(&flight).await {
                FlightOutcome::Answered(answer) => return answer,
                FlightOutcome::Abandoned => continue,
            },
        }
    }
}

/// `Some(Some(bytes))` is a hit, `Some(None)` a remembered absence, `None` a
/// miss. Bytes first: the common case, and the more valuable answer.
fn lookup(k: &str) -> Option<Option<Bytes>> {
    if let Some(hit) = byte_cache().lock().get(k) {
        return Some(Some(hit));
    }
    if absent_cache().lock().get(k).is_some() {
        return Some(None);
    }
    None
}

/// Record a definitive answer. A store error never reaches here, so a
/// transient failure doesn't pin a false absence for the life of the build.
fn remember(k: String, app_id: Uuid, build_id: &str, rel_path: &str, outcome: &Option<Bytes>) {
    match outcome {
        Some(bytes) => {
            if !byte_cache().lock().put(k, bytes.clone()) {
                // Served, not remembered. Logged at `debug` rather than
                // `trace` — unlike the long-path case below this is bounded by
                // what an app actually publishes, so it cannot be driven by a
                // scanner, and an operator wondering why one asset never warms
                // needs a line to find.
                tracing::debug!(
                    target: "oxy.custom_app.bundle_cache",
                    app_id = %app_id,
                    build_id,
                    bytes = bytes.len(),
                    "object exceeds the per-entry cache ceiling; serving without caching"
                );
            }
        }
        None if rel_path.len() <= MAX_ABSENT_REL_LEN => {
            absent_cache().lock().put(k, ());
        }
        // Too long to be worth remembering — see `MAX_ABSENT_REL_LEN`.
        // Logged because the consequence is a store round-trip on *every*
        // request for this path: without a line to grep, that surfaces only
        // as unexplained store traffic.
        //
        // `trace!`, not `debug!`: these are exactly the paths with no
        // memoization, so the line repeats per request rather than once —
        // a scanner walking long URLs would hold it open indefinitely, and
        // `debug` is enabled in plenty of dev and staging configs.
        //
        // A 64-char prefix rather than the whole path: an operator needs a
        // handle to correlate against the access log, but `rel_path` is
        // request-controlled and 512+ bytes of it per line is its own
        // problem. Taken by `chars()` so a multi-byte boundary can't panic.
        None => {
            let prefix: String = rel_path.chars().take(64).collect();
            tracing::trace!(
                "app {app_id} build {build_id}: not caching absence of a {} byte path \
                 (> {MAX_ABSENT_REL_LEN}); every request for it hits the store. starts: {prefix:?}",
                rel_path.len()
            );
        }
    }
}

/// Warm the cache directly (used at publish time so the first viewer of a
/// freshly-published `index.html` doesn't pay the cold S3 round trip).
pub fn seed(app_id: Uuid, build_id: &str, rel_path: &str, bytes: Bytes) {
    let k = key(app_id, build_id, rel_path);
    // Drop any recorded absence for this key. Lookups check bytes first, so
    // this isn't load-bearing for correctness — but leaving a contradicted
    // negative behind wastes a slot and would read as a bug to the next
    // person holding both maps in their head.
    absent_cache().lock().pop(&k);
    byte_cache().lock().put(k, bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Eight concurrent misses on one cold key reach the store ONCE. The
    /// fake store blocks until the test releases it, so the assertion is on
    /// a condition and not a race: while it is held, nobody can have been
    /// answered (the only answer is the store's), and after it is released
    /// everyone is answered from the leader's single fetch.
    #[tokio::test]
    async fn concurrent_misses_on_one_key_fetch_the_store_once() {
        let app = Uuid::new_v4();
        let calls = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(tokio::sync::Notify::new());

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let calls = calls.clone();
                let gate = gate.clone();
                tokio::spawn(async move {
                    get_or_fetch_with(app, "b1", "assets/app.js", move || async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        gate.notified().await;
                        Ok(Some(Bytes::from_static(b"js")))
                    })
                    .await
                })
            })
            .collect();

        // Let the leader reach the store and every follower reach the gate.
        while calls.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        for _ in 0..64 {
            tokio::task::yield_now().await;
        }
        assert!(
            handles.iter().all(|h| !h.is_finished()),
            "nobody can be answered while the one store call is still held"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1, "followers must not fetch");

        // `notify_one` stores a permit if the leader is not yet parked.
        gate.notify_one();
        for h in handles {
            let answer = h.await.expect("task").expect("store answered");
            assert_eq!(answer.as_deref(), Some(&b"js"[..]));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(
            in_flight()
                .lock()
                .get(&key(app, "b1", "assets/app.js"))
                .is_none(),
            "the flight is released once the leader is done"
        );
    }

    /// A leader whose request is cancelled mid-fetch must not leave the key
    /// gated forever. The follower that was waiting on it takes over and
    /// fetches; the entry is gone afterwards.
    #[tokio::test]
    async fn a_cancelled_leader_hands_the_key_to_the_next_caller() {
        let app = Uuid::new_v4();
        let started = Arc::new(AtomicUsize::new(0));
        let hold = Arc::new(tokio::sync::Notify::new());
        let leader = tokio::spawn({
            let started = started.clone();
            let hold = hold.clone();
            async move {
                get_or_fetch_with(app, "b1", "assets/x.js", move || async move {
                    started.fetch_add(1, Ordering::SeqCst);
                    // Never released: this request is going to be cancelled.
                    hold.notified().await;
                    Ok(None)
                })
                .await
            }
        });
        while started.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let follower = tokio::spawn({
            let calls = calls.clone();
            async move {
                get_or_fetch_with(app, "b1", "assets/x.js", move || async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(Some(Bytes::from_static(b"js")))
                })
                .await
            }
        });
        for _ in 0..32 {
            tokio::task::yield_now().await;
        }
        assert!(!follower.is_finished(), "the follower waits on the leader");

        leader.abort();
        let _ = leader.await;

        let answer = tokio::time::timeout(std::time::Duration::from_secs(5), follower)
            .await
            .expect("the follower is released, not left on a dead gate")
            .expect("task")
            .expect("the follower's own fetch");
        assert_eq!(answer.as_deref(), Some(&b"js"[..]));
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "the follower led the retry"
        );
        assert!(
            in_flight()
                .lock()
                .get(&key(app, "b1", "assets/x.js"))
                .is_none()
        );
    }

    /// Under a store outage the leader's error is every follower's error —
    /// one failed round trip, not one per waiter — and the next request
    /// after the flight is released tries the store afresh.
    #[tokio::test]
    async fn a_leaders_error_is_shared_with_every_follower_without_more_store_calls() {
        let app = Uuid::new_v4();
        let calls = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(tokio::sync::Notify::new());
        let leader = tokio::spawn({
            let calls = calls.clone();
            let release = release.clone();
            async move {
                get_or_fetch_with(app, "b1", "assets/y.js", move || async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    release.notified().await;
                    Err(BuildStoreError::S3("outage".into()))
                })
                .await
            }
        });
        while calls.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        let followers: Vec<_> = (0..4)
            .map(|_| {
                let calls = calls.clone();
                tokio::spawn(async move {
                    get_or_fetch_with(app, "b1", "assets/y.js", move || async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        Ok(Some(Bytes::from_static(b"must not run")))
                    })
                    .await
                })
            })
            .collect();
        for _ in 0..32 {
            tokio::task::yield_now().await;
        }
        release.notify_one();

        assert!(leader.await.expect("task").is_err());
        for follower in followers {
            assert!(
                follower.await.expect("task").is_err(),
                "a follower fails fast with the leader's error"
            );
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "one store call for the whole flight"
        );

        // The error was not cached: the next caller leads a fresh flight.
        let again = get_or_fetch_with(app, "b1", "assets/y.js", {
            let calls = calls.clone();
            move || async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(Some(Bytes::from_static(b"ok")))
            }
        })
        .await
        .expect("the store recovered");
        assert_eq!(again.as_deref(), Some(&b"ok"[..]));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    /// A store error is returned to the caller that saw it and NOT cached;
    /// the next caller tries again — single-flight must not turn one blip
    /// into a cached failure.
    #[tokio::test]
    async fn a_store_error_is_not_cached_and_the_next_caller_retries() {
        let app = Uuid::new_v4();
        let first = get_or_fetch_with(app, "b1", "assets/x.js", || async {
            Err(BuildStoreError::S3("blip".into()))
        })
        .await;
        assert!(first.is_err());
        let second = get_or_fetch_with(app, "b1", "assets/x.js", || async {
            Ok(Some(Bytes::from_static(b"ok")))
        })
        .await
        .expect("second fetch runs");
        assert_eq!(second.as_deref(), Some(&b"ok"[..]));
    }

    /// The absence itself must be remembered. Proven observably: ask for a
    /// file that isn't there (caching the absence), then create it on disk
    /// and ask again — a still-`None` answer can only mean the second call
    /// never reached the store.
    ///
    /// This is what spares the SPA-fallback and `.br`-probe paths a doomed
    /// store round-trip on every single request.
    #[tokio::test]
    async fn absent_object_is_remembered_and_not_refetched() {
        let tmp = std::env::temp_dir().join(format!("oxy-bc-test-{}", Uuid::new_v4()));
        // SAFETY: nextest runs each test in its own process, so no other test
        // observes these vars. Forces the filesystem build-store backend.
        unsafe {
            std::env::remove_var("OXY_CUSTOMER_APPS_S3_BUCKET");
            std::env::set_var("OXY_STATE_DIR", &tmp);
        }
        let app = Uuid::new_v4();

        let first = get_or_fetch(app, "b1", "assets/main.js.br")
            .await
            .expect("miss is not an error");
        assert!(first.is_none(), "object genuinely absent on the first ask");

        // Materialise the file the cache has already recorded as absent.
        let dir = tmp.join(format!("customer-apps/{app}/builds/b1/assets"));
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("main.js.br"), b"\x1b\x0e\x00").expect("write");

        let second = get_or_fetch(app, "b1", "assets/main.js.br")
            .await
            .expect("cached absence is not an error");
        assert!(
            second.is_none(),
            "absence must be served from cache — a Some here means the store was hit again"
        );

        unsafe {
            std::env::remove_var("OXY_STATE_DIR");
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Absences must not be able to push bytes out. The two live in separate
    /// LRUs precisely so a crawler walking one app's client-side routes
    /// can't cost another app its hot assets on this process-global cache.
    #[tokio::test]
    async fn absences_cannot_evict_cached_bytes() {
        // SAFETY: nextest gives each test its own process.
        unsafe {
            std::env::remove_var("OXY_CUSTOMER_APPS_S3_BUCKET");
            std::env::set_var("OXY_STATE_DIR", std::env::temp_dir().join("oxy-bc-evict"));
        }
        let victim = Uuid::new_v4();
        let bytes = Bytes::from_static(b"<html>hot asset");
        seed(victim, "b1", "assets/hot.js", bytes.clone());

        // Record more absences than the byte cache could ever hold. Under one
        // shared map this alone would evict the seeded entry.
        //
        // `expect` rather than `let _`: errors are deliberately not cached
        // (see `get_or_fetch`), so a loop that errored every iteration would
        // record zero absences, never pressure the byte cache, and leave the
        // assertion below passing vacuously.
        let noisy = Uuid::new_v4();
        for i in 0..(MAX_BYTE_ENTRIES + 256) {
            let miss = get_or_fetch(noisy, "b1", &format!("route/{i}"))
                .await
                .expect("a miss is not an error — the premise of this test");
            assert!(miss.is_none(), "route/{i} must not exist");
        }

        let got = get_or_fetch(victim, "b1", "assets/hot.js")
            .await
            .expect("cached bytes must not require the store");
        assert_eq!(
            got.as_deref(),
            Some(&b"<html>hot asset"[..]),
            "negative entries evicted cached bytes — the two caches are sharing capacity"
        );
        unsafe {
            std::env::remove_var("OXY_STATE_DIR");
        }
    }

    /// The invariant the whole design rests on: `resident` is the sum of the
    /// entries' lengths. These exercise `ByteBudgetCache` directly rather than
    /// through `get_or_fetch`, because the budget is process-global and
    /// resolved once — a test that drove it through the real cache could not
    /// choose a budget small enough to make evictions observable.
    #[test]
    fn resident_tracks_the_sum_of_entries() {
        let mut c = ByteBudgetCache::with_budget(1_000_000);
        assert_eq!(c.resident, 0);

        c.put("a".into(), Bytes::from_static(&[0u8; 100]));
        c.put("b".into(), Bytes::from_static(&[0u8; 250]));
        assert_eq!(c.resident, 350);

        c.pop("a");
        assert_eq!(c.resident, 250, "pop must release the entry's bytes");

        c.pop("nonexistent");
        assert_eq!(c.resident, 250, "popping a missing key changes nothing");
    }

    /// **The bug this shape invites.** `LruCache::put` returns the displaced
    /// value; forgetting to subtract its length leaks budget on every
    /// overwrite, until the cache believes it is full while holding almost
    /// nothing and evicts everything on the next insert.
    ///
    /// Overwrites are not hypothetical here: a re-`seed` of `index.html` at
    /// publish time hits exactly this path.
    #[test]
    fn replacing_an_entry_does_not_leak_budget() {
        let mut c = ByteBudgetCache::with_budget(1_000_000);
        c.put("k".into(), Bytes::from_static(&[0u8; 1000]));
        assert_eq!(c.resident, 1000);

        c.put("k".into(), Bytes::from_static(&[0u8; 10]));
        assert_eq!(
            c.resident, 10,
            "the displaced value's bytes must be released, not added to"
        );

        c.put("k".into(), Bytes::from_static(&[0u8; 400]));
        assert_eq!(c.resident, 400);
        assert_eq!(c.lru.len(), 1, "an overwrite is not a second entry");
    }

    /// An object larger than the per-entry ceiling is refused rather than
    /// admitted-and-immediately-evicted. Admitting it would evict a large
    /// share of the cache to hold something that is itself next to go.
    #[test]
    fn an_oversized_object_is_refused_and_leaves_the_cache_intact() {
        let mut c = ByteBudgetCache::with_budget(8_000);
        c.put("hot".into(), Bytes::from_static(b"keep me"));
        let before = c.resident;

        let huge = Bytes::from(vec![0u8; 8_000 / OVERSIZE_DIVISOR + 1]);
        assert!(
            !c.put("huge".into(), huge),
            "an object past the per-entry ceiling must be refused"
        );
        assert_eq!(c.resident, before, "a refused object contributes no bytes");
        assert!(
            c.get("hot").is_some(),
            "a refused object must not have evicted anything"
        );
    }

    /// An object exactly at the ceiling is admitted — the bound is `>`, not
    /// `>=`, and an off-by-one here silently halves the useful entry size.
    #[test]
    fn an_object_exactly_at_the_ceiling_is_admitted() {
        let mut c = ByteBudgetCache::with_budget(8_000);
        let at_limit = Bytes::from(vec![0u8; 8_000 / OVERSIZE_DIVISOR]);
        assert!(c.put("edge".into(), at_limit));
        assert!(c.get("edge").is_some());
    }

    /// The point of the change: the cache evicts on **bytes**, long before the
    /// 8192-entry cap would bind.
    ///
    /// Sizing note — entries must clear the per-entry ceiling
    /// (`budget / OVERSIZE_DIVISOR`) or they are refused rather than admitted
    /// and evicted, and the test would pass vacuously with an empty cache.
    /// 1 KiB against a 10 KiB budget gives a 1.25 KiB ceiling, so all fifteen
    /// are admissible and only the byte budget decides what stays.
    #[test]
    fn the_budget_evicts_least_recently_used_bytes() {
        let mut c = ByteBudgetCache::with_budget(10_000);
        for i in 0..15 {
            assert!(
                c.put(format!("k{i}"), Bytes::from(vec![0u8; 1_000])),
                "k{i} must be admissible, or this test proves nothing"
            );
        }
        assert!(
            c.resident <= 10_000,
            "resident {} exceeded the 10000-byte budget",
            c.resident
        );
        assert!(
            c.lru.len() < 15,
            "nothing was evicted — the cache is still count-bounded only"
        );
        assert_eq!(
            c.resident,
            c.lru.iter().map(|(_, v)| v.len()).sum::<usize>(),
            "the running total drifted from the map it describes"
        );
        assert!(
            c.get("k0").is_none(),
            "eviction must take the LEAST recently used first"
        );
        assert!(c.get("k14").is_some(), "the newest entry must survive");
    }

    /// Every other test here injects its caps, which is what makes the
    /// eviction paths cheap to exercise — and leaves the *production* wiring
    /// untested. This is the one assertion that the cache a real replica builds
    /// carries the real bounds; without it, a `new()` that silently stopped
    /// passing `MAX_BYTE_ENTRIES` would keep the whole suite green.
    #[test]
    fn the_default_constructor_wires_the_real_caps() {
        let c = ByteBudgetCache::new();
        assert_eq!(
            c.lru.cap().get(),
            MAX_BYTE_ENTRIES,
            "the entry cap must come from the constant, not a test value"
        );
        // Deliberately compared against `byte_budget()` rather than
        // `MAX_BYTE_CACHE_BYTES`: the contract is "new() reads the resolved
        // global", and asserting the constant would make this test fail
        // whenever OXY_BUNDLE_CACHE_MAX_MB is set in the environment. It does
        // NOT pin what the global resolves to — `resolve_budget_publishes_the_gauge`
        // below covers that half.
        assert_eq!(
            c.budget,
            byte_budget(),
            "the byte budget must come from the resolved global"
        );
    }

    /// The gauge contract the boot call exists to satisfy.
    ///
    /// `resolve_budget()` is only useful if calling it actually publishes
    /// `oxy_custom_app_bundle_cache_limit_bytes` — and for a long time the boot
    /// site did not call it at all, because the call sat inside a
    /// `tracing::info!` field expression and `tracing` evaluates those only when
    /// the callsite is enabled. Default `OXY_LOG_LEVEL` is `warn`, so it never
    /// ran, and the gauge stayed at the value that also means "disabled".
    ///
    /// This pins the half that is testable here: resolving publishes. That the
    /// *boot path* resolves is now a plain `let` in `serve.rs`, which cannot be
    /// optimised away by a log level.
    #[test]
    fn resolve_budget_publishes_the_gauge() {
        use std::sync::atomic::Ordering;

        // nextest gives each test its own process, so the OnceLock is ours and
        // the env is uncontended.
        unsafe { std::env::set_var(MAX_BYTE_CACHE_BYTES_ENV, "64") };

        assert_eq!(
            oxy_telemetry::metrics::sources::BUNDLE_CACHE_LIMIT.load(Ordering::Relaxed),
            0,
            "nothing should have resolved the budget yet in this process"
        );

        let resolved = resolve_budget();
        assert_eq!(resolved, 64 * 1024 * 1024);
        assert_eq!(
            oxy_telemetry::metrics::sources::BUNDLE_CACHE_LIMIT.load(Ordering::Relaxed),
            resolved as u64,
            "resolving the budget must publish it — a gauge left at 0 is \
             indistinguishable from the byte bound being disabled"
        );

        unsafe { std::env::remove_var(MAX_BYTE_CACHE_BYTES_ENV) };
    }

    /// **The capacity-eviction twin of the replacement test**, and the case
    /// that was wrong.
    ///
    /// `LruCache::put` returns *only* a replaced value: when the map is at
    /// capacity it evicts the least-recently-used entry internally and returns
    /// `None`, because the victim is a different key than the one inserted and
    /// the `Option<V>` signature cannot name it. So every insert past the cap
    /// added its bytes with no matching subtraction and `resident` drifted
    /// upward forever — over-reporting the gauge, then evicting real entries to
    /// pay down phantom bytes, converging on evict-per-insert.
    ///
    /// The entry cap is not the unreachable bound here: it binds first whenever
    /// the mean object is under `budget / entries`, which is most of a Vite
    /// build on a replica hosting many apps — the exact scenario this module
    /// exists for.
    #[test]
    fn capacity_eviction_keeps_the_byte_accounting_honest() {
        // Tiny entry cap, budget large enough that it never binds — so this
        // isolates the count path.
        let mut c = ByteBudgetCache::with_caps(4, 10_000_000);
        for i in 0..40 {
            assert!(c.put(format!("k{i}"), Bytes::from(vec![0u8; 100])));
        }

        assert_eq!(c.lru.len(), 4, "the entry cap must still bind");
        assert_eq!(
            c.resident,
            c.lru.iter().map(|(_, v)| v.len()).sum::<usize>(),
            "resident drifted from the map: a capacity eviction's bytes were \
             never released (LruCache::put discards the victim — use push)"
        );
        assert_eq!(c.resident, 400, "four 100-byte entries");
    }

    /// Both caps binding at once, which is the production shape.
    #[test]
    fn accounting_holds_when_both_caps_bind() {
        let mut c = ByteBudgetCache::with_caps(8, 1_000);
        for i in 0..50 {
            c.put(format!("k{i}"), Bytes::from(vec![0u8; 120]));
        }
        assert!(c.lru.len() <= 8);
        assert!(c.resident <= 1_000);
        assert_eq!(
            c.resident,
            c.lru.iter().map(|(_, v)| v.len()).sum::<usize>(),
            "resident must equal the map under both bounds"
        );
    }

    /// A zero budget disables the byte bound entirely, leaving the original
    /// entry cap — the documented off switch, and the rollback path.
    #[test]
    fn a_zero_budget_disables_the_byte_bound() {
        let mut c = ByteBudgetCache::with_budget(0);
        for i in 0..20 {
            c.put(format!("k{i}"), Bytes::from(vec![0u8; 10_000]));
        }
        assert_eq!(
            c.lru.len(),
            20,
            "with the bound off, nothing evicts on bytes"
        );
        assert_eq!(
            c.resident, 200_000,
            "the total is still tracked, just not enforced"
        );
    }

    #[tokio::test]
    async fn seeded_entry_served_without_s3() {
        // Seeding first proves the cache short-circuits the store entirely:
        // the lookup returns the seeded bytes without ever calling
        // get_object (no S3 round trip, no filesystem read).
        let app = Uuid::new_v4();
        let bytes = Bytes::from_static(b"<html>seeded");
        seed(app, "bx", "index.html", bytes.clone());
        let got = get_or_fetch(app, "bx", "index.html")
            .await
            .expect("cache hit must not touch S3");
        assert_eq!(got.as_deref(), Some(&b"<html>seeded"[..]));
    }
}
