//! Process-wide cache for DuckDB connections.
//!
//! Before this pool, [`super::duckdb::DuckDB::run_query_with_limit`] called
//! `init_connection` on every query, which:
//!   1. Opened a fresh `Connection::open_in_memory()`, and
//!   2. Re-parsed every CSV/Parquet file in the dataset directory into a
//!      temporary table.
//!
//! For a 50 MB CSV that's ~9 seconds per query — multiplied by every
//! `execute_sql` task in an automation run, including dozens of tasks that exist
//! only inside nested sub-automations. Automations that should run in seconds were
//! taking tens of minutes.
//!
//! The pool keeps **one** primary connection per [`PoolTarget`] (i.e. one per
//! logical "database") alive for the lifetime of the process. Each query
//! checks out a fresh connection via `try_clone()`, which shares the
//! underlying database with the primary (so the loaded tables are visible)
//! but has its own statement cache and transaction state. Tables are loaded
//! as regular `CREATE TABLE` (not `TEMPORARY`) so cloned connections see
//! them; they live only in the in-memory database and disappear when the
//! primary is dropped.
//!
//! Cache invalidation is keyed on file mtime. Each cached entry remembers
//! the `PoolKey` it was built for; on lookup, if the freshly-computed
//! `PoolKey` doesn't match the cached one we drop the stale entry and
//! rebuild. Crucially we keep at most one entry per `PoolTarget`, so the
//! map cannot grow unboundedly across mtime generations — the previous
//! entry's `Arc<PoolEntry>` is dropped on insert, releasing the in-memory
//! database (a non-trivial amount of RAM for large CSVs).
//!
//! # That bound is per *target*, and a target can be disposable
//!
//! "One entry per target" bounds the map only while targets recur. A caller
//! that mints a **fresh** target per unit of work — the simulation runner
//! materialises each run's dataset into its own `TempDir` — gets one slot per
//! run, each pinning a live in-memory database and every table materialised
//! into it, keyed on a directory that no longer exists. Nothing evicts them:
//! there is no capacity bound, no TTL, and eviction is same-key replacement by
//! a key that never recurs.
//!
//! Such a caller must hand the target back with [`DuckDBPool::release`] when
//! its lifetime ends — ideally from the `Drop` of whatever owns the directory,
//! so an early return or a panic releases it too. `release` clears the
//! `init_locks` entry as well, which [`DuckDBPool::invalidate`] deliberately
//! does not.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use duckdb::Connection;

use crate::connector::constants::CREATE_CONN;
use crate::connector::utils::connector_internal_error;
use oxy_shared::errors::OxyError;

/// What kind of DuckDB target the pooled handle wraps. There is at most one
/// pooled entry per target — invalidation replaces, never accumulates.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum PoolTarget {
    /// Local mode: an in-memory DuckDB pre-loaded with one table per file in
    /// `dir`.
    Local { dir: PathBuf },
    /// File mode: an on-disk DuckDB database.
    File { path: PathBuf },
    /// A remote MotherDuck database (`md:<database>`). Identified by database
    /// name only — the credential is a *generation* of that identity and lives in
    /// [`PoolKey::credential`], exactly as a file's mtime does, so a rotated token
    /// replaces the slot instead of accumulating one per token.
    ///
    /// **Caveat:** the *account* is not part of the identity, because nothing
    /// short of the token identifies it and folding the token in would turn a
    /// rotation into a new slot rather than a replacement. Two accounts using the
    /// same database name — including the very common `database: None` (`md:`) —
    /// therefore share one slot with different credentials, and each query evicts
    /// the other's handle, degrading to the per-query opens this pool replaced.
    /// Correctness is unaffected: `lookup_fresh` compares the whole key, including
    /// the credential, so a session is never reused across accounts. Prod has a
    /// single MotherDuck workspace today, so this is latent rather than live; if
    /// that changes, key on an account identifier rather than the token.
    MotherDuck { database: Option<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PoolKey {
    target: PoolTarget,
    /// Sorted `(path, mtime_secs, mtime_nanos)` for every file the handle
    /// depends on. `mtime` is captured as a tuple of `u64` + `u32` so the
    /// key is hashable / comparable (`SystemTime` is not).
    file_signatures: Vec<(PathBuf, u64, u32)>,
    /// Fingerprint of the credential a token-authenticated handle was opened
    /// with; `None` for targets whose freshness is decided by file mtimes.
    ///
    /// Only the fingerprint, never the token: pool keys sit in a process-global
    /// map for the lifetime of the process, and a token there would outlive every
    /// scope that was supposed to own it.
    credential: Option<u64>,
}

impl PoolKey {
    /// Key for a MotherDuck database. `token` is fingerprinted, not stored.
    ///
    /// SHA-256 truncated to 64 bits rather than `DefaultHasher`: the fingerprint
    /// decides whether a cached session may be served, so a collision would hand
    /// out a handle opened under a *different* credential. `DefaultHasher` is a
    /// non-cryptographic digest with no collision resistance against a chosen
    /// input; `sha2` is already a dependency, so the stronger digest is free.
    pub(super) fn motherduck(database: Option<&str>, token: &str) -> Self {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(token.as_bytes());
        let mut head = [0u8; 8];
        head.copy_from_slice(&digest[..8]);
        PoolKey {
            target: PoolTarget::MotherDuck {
                database: database.map(str::to_owned),
            },
            file_signatures: Vec::new(),
            credential: Some(u64::from_be_bytes(head)),
        }
    }

    pub(super) fn local(dir: PathBuf, files: &[(String, PathBuf)]) -> Result<Self, OxyError> {
        let mut signatures = Vec::with_capacity(files.len() + 1);
        // Include the directory itself so a `.csv` rename (which preserves
        // file mtimes but changes the directory listing) still busts the key.
        signatures.push(file_signature(&dir)?);
        for (_, path) in files {
            signatures.push(file_signature(path)?);
        }
        signatures.sort();
        Ok(PoolKey {
            target: PoolTarget::Local { dir },
            file_signatures: signatures,
            credential: None,
        })
    }

    pub(super) fn file(path: PathBuf) -> Result<Self, OxyError> {
        // Canonicalize so two callers passing the same on-disk file via
        // different path representations (relative vs. absolute, symlink
        // vs. resolved target, with or without trailing `.`) collapse to
        // the same `PoolTarget`. `canonicalize` requires the file to exist;
        // if DuckDB will create it on open, fall back to the raw path —
        // subsequent calls converge once the file is materialized.
        let canonical = path.canonicalize().unwrap_or(path);
        let signatures = match file_signature(&canonical) {
            Ok(sig) => vec![sig],
            // File may not exist yet — DuckDB will create it on open. Use a
            // zero signature so subsequent calls hit the same key until the
            // file is actually created.
            Err(e) => {
                tracing::warn!(
                    path = %canonical.display(),
                    error = %e,
                    "DuckDB pool: file-stat failed; pool invalidation disabled for this path until stat succeeds"
                );
                vec![(canonical.clone(), 0, 0)]
            }
        };
        Ok(PoolKey {
            target: PoolTarget::File { path: canonical },
            file_signatures: signatures,
            credential: None,
        })
    }
}

fn file_signature(path: &Path) -> Result<(PathBuf, u64, u32), OxyError> {
    let meta = std::fs::metadata(path).map_err(|e| {
        OxyError::DBError(format!(
            "DuckDB pool: cannot stat '{}': {e}",
            path.display()
        ))
    })?;
    let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let dur = mtime
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    Ok((path.to_path_buf(), dur.as_secs(), dur.subsec_nanos()))
}

/// One pooled DuckDB instance. Holds the primary connection alive so cloned
/// connections from `try_clone()` keep seeing the loaded data.
///
/// `session_setup` are statements that must be re-run on every cloned
/// connection because they configure per-session state (e.g. `SET
/// file_search_path`, `LOAD icu`). Cloned connections share the database
/// catalog and tables but get a fresh session, so settings don't carry over.
pub(super) struct PoolEntry {
    /// `std::sync::Mutex` rather than `tokio::sync::Mutex`: `try_clone()` is
    /// a millisecond-scale operation, callers don't `.await` while holding
    /// the guard, and DuckDB's own internal scheduler handles cross-thread
    /// query parallelism.
    primary: Mutex<Connection>,
    session_setup: Vec<String>,
}

impl PoolEntry {
    /// Hand out a fresh connection that shares the underlying database with
    /// `primary`. The returned connection has its own statement cache and
    /// can be dropped at end of query without losing any loaded tables.
    pub(super) fn checkout(&self) -> Result<Connection, OxyError> {
        let primary = self
            .primary
            .lock()
            .expect("DuckDB pool primary mutex poisoned");
        let conn = primary
            .try_clone()
            .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
        drop(primary);
        for stmt in &self.session_setup {
            conn.execute(stmt, [])
                .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
        }
        Ok(conn)
    }
}

/// One slot in the pool: the `PoolEntry` plus the `PoolKey` it was built for.
/// On lookup we compare the freshly-computed key against the stored one to
/// detect mtime changes; on mismatch we drop the slot and rebuild.
struct Slot {
    key: PoolKey,
    entry: Arc<PoolEntry>,
}

/// Process-wide singleton pool. Indexed by [`PoolTarget`] (one slot per
/// logical database) so an mtime change replaces the slot rather than
/// accumulating beside it. The replaced slot's `Arc<PoolEntry>` drops once
/// the last in-flight checkout returns, releasing the in-memory database.
///
/// `init_locks` is a per-target mutex that serialises concurrent initialisations
/// for the *same* target. Without it, N callers that all miss the cache
/// simultaneously each run `init` (loading every CSV/Parquet file into a
/// separate in-memory database), paying the full initialisation cost N times
/// before one slot wins and N-1 are thrown away. With the per-target lock, only
/// the first caller runs `init`; the rest wait and then hit the warm cache.
/// Callers for *different* targets are unaffected — they hold different locks.
pub(super) struct DuckDBPool {
    slots: Mutex<HashMap<PoolTarget, Slot>>,
    init_locks: Mutex<HashMap<PoolTarget, Arc<Mutex<()>>>>,
}

impl Default for DuckDBPool {
    fn default() -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
            init_locks: Mutex::new(HashMap::new()),
        }
    }
}

pub(super) fn pool() -> &'static DuckDBPool {
    static POOL: OnceLock<DuckDBPool> = OnceLock::new();
    POOL.get_or_init(DuckDBPool::default)
}

impl DuckDBPool {
    /// Look up the pooled entry for `key.target`. If the cached slot's
    /// `PoolKey` matches `key`, return its entry. If it differs (mtime
    /// changed) or no slot exists, build via `init` and replace.
    ///
    /// Concurrent callers for the **same** target serialise on a per-target
    /// init lock: only the first caller runs `init`; the rest wait and then
    /// return the entry the first caller inserted (double-checked locking).
    /// Concurrent callers for **different** targets are never blocked by each
    /// other — they acquire different per-target locks.
    pub(super) fn get_or_init<F>(&self, key: PoolKey, init: F) -> Result<Arc<PoolEntry>, OxyError>
    where
        F: FnOnce() -> Result<(Connection, Vec<String>), OxyError>,
    {
        // Fast path: warm cache hit — no locking beyond the slots map read.
        if let Some(entry) = self.lookup_fresh(&key) {
            return Ok(entry);
        }

        // Acquire (or create) the per-target init lock before running `init`.
        // This mutex is held for the duration of `init` so that a second caller
        // for the same target waits here rather than starting its own `init`.
        let init_lock = {
            let mut locks = self
                .init_locks
                .lock()
                .expect("DuckDB pool init_locks mutex poisoned");
            locks
                .entry(key.target.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _init_guard = init_lock
            .lock()
            .expect("DuckDB pool per-target init mutex poisoned");

        // Double-check: a concurrent caller may have initialised while we waited.
        if let Some(entry) = self.lookup_fresh(&key) {
            return Ok(entry);
        }

        let (conn, session_setup) = init()?;
        let new_entry = Arc::new(PoolEntry {
            primary: Mutex::new(conn),
            session_setup,
        });
        let target = key.target.clone();
        let mut slots = self.slots.lock().expect("DuckDB pool slots mutex poisoned");
        // `insert` returns the previously-stored slot (if any); dropping it
        // here releases the stale `PoolEntry`'s primary connection — i.e. the
        // in-memory database tied to it. This is the eviction.
        let _previous = slots.insert(
            target,
            Slot {
                key,
                entry: new_entry.clone(),
            },
        );
        Ok(new_entry)
    }

    /// Drop the cached entry for `target`, so the next [`Self::get_or_init`]
    /// rebuilds it from scratch.
    ///
    /// Mtime-based eviction cannot express "the handle itself went bad", which is
    /// a failure mode only the network-backed targets have: a `Local`/`File`
    /// primary is in-process and stays valid until dropped, whereas a MotherDuck
    /// primary is a session the server can end at any time. Callers invoke this
    /// when a query fails in a way that implicates the handle rather than the SQL.
    ///
    /// Two properties this deliberately does **not** have. It only unlinks the
    /// slot, so a query running concurrently on another thread keeps the clone it
    /// already checked out and the next caller opens a second primary alongside
    /// it — briefly more than one handle per database, though never a concurrent
    /// *init* (`get_or_init` still serialises those on the per-target lock), which
    /// is the shape that actually SIGSEGVs. And it inherits the name-only identity
    /// documented on [`PoolTarget::MotherDuck`], so where two accounts share a
    /// database name, one account's failed query drops the other's slot — a
    /// wasted reopen, not a correctness problem, since the key comparison still
    /// includes the credential.
    ///
    /// Note that the target's entry in `init_locks` is deliberately left behind:
    /// a caller may be waiting on it right now, and the map holds one small
    /// `Arc<Mutex<()>>` per target for the life of the process — bounded by the
    /// number of distinct targets, not by how often they are invalidated.
    pub(super) fn invalidate(&self, target: &PoolTarget) {
        let mut slots = self.slots.lock().expect("DuckDB pool slots mutex poisoned");
        slots.remove(target);
    }

    /// Drop **everything** the pool holds for `target` — the slot and the
    /// entry in `init_locks` — because the target itself is gone.
    ///
    /// This is the counterpart to a *disposable* target: a dataset directory
    /// that lives for one run (the simulation runner materialises one per run
    /// in a `TempDir`) is a `PoolTarget` nobody will ever check out again.
    /// [`Self::invalidate`] is the wrong tool there — it deliberately keeps the
    /// init-lock entry, on the grounds that the map is "bounded by the number
    /// of distinct targets", and that is exactly the assumption a per-run
    /// target breaks. Left to the ordinary mechanisms the slot would never be
    /// evicted at all: eviction is same-key replacement, and the key never
    /// recurs.
    ///
    /// Callers must only use this when they *own* the target's lifetime and it
    /// has ended. For a target that may be checked out again, use
    /// [`Self::invalidate`]: the next `get_or_init` there rebuilds, whereas
    /// removing the init lock out from under a live target reopens the
    /// duplicate-init window that lock exists to close.
    ///
    /// # The init-lock waiter hazard
    ///
    /// A caller in [`Self::get_or_init`] clones the target's `Arc<Mutex<()>>`
    /// out of `init_locks` and *then* blocks on it. If we removed the map's
    /// copy in between, a later caller would mint a second, unrelated lock and
    /// both would run `init` concurrently — the duplicate-init window the lock
    /// closes. So the removal is conditional: the map's `Arc` is only dropped
    /// when it is the sole strong reference. The check is sound because we hold
    /// the `init_locks` mutex while making it, and the map is the only source
    /// of clones — so no new reference can appear between the count and the
    /// removal, and after the removal none can ever be minted from that entry.
    ///
    /// The slot is removed first. A caller racing us can then re-insert a slot
    /// (it is mid-`init`), but that same caller necessarily holds a clone of
    /// the init lock, so the count check leaves the lock in place and the two
    /// maps stay consistent with each other.
    pub(super) fn release(&self, target: &PoolTarget) {
        {
            let mut slots = self.slots.lock().expect("DuckDB pool slots mutex poisoned");
            slots.remove(target);
        }
        let mut locks = self
            .init_locks
            .lock()
            .expect("DuckDB pool init_locks mutex poisoned");
        let uncontended = locks
            .get(target)
            .is_some_and(|lock| Arc::strong_count(lock) == 1);
        if uncontended {
            locks.remove(target);
        } else if locks.contains_key(target) {
            // Not a leak worth failing over — one `Arc<Mutex<()>>` — but it is
            // the shape that would make this method stop bounding the map, and
            // it should not be reachable for a target whose lifetime the caller
            // owns. Worth seeing if it ever happens.
            tracing::debug!(
                ?target,
                "DuckDB pool: released a target whose init lock still had a waiter; \
                 leaving the lock entry in place"
            );
        }
    }

    /// Test-only visibility into the two maps. The invariant this module
    /// asserts is about their *size*, so a leak is only assertable by counting
    /// them — and neither map has (or should have) a production reader.
    #[cfg(test)]
    pub(super) fn slot_count(&self) -> usize {
        self.slots.lock().expect("slots mutex poisoned").len()
    }

    #[cfg(test)]
    pub(super) fn init_lock_count(&self) -> usize {
        self.init_locks
            .lock()
            .expect("init_locks mutex poisoned")
            .len()
    }

    #[cfg(test)]
    pub(super) fn holds_slot(&self, target: &PoolTarget) -> bool {
        self.slots
            .lock()
            .expect("slots mutex poisoned")
            .contains_key(target)
    }

    /// Take the clone of a target's init lock that a caller inside
    /// [`Self::get_or_init`] would be holding, so a test can stand in for the
    /// waiter [`Self::release`]'s count check is guarding against.
    #[cfg(test)]
    pub(super) fn clone_init_lock(&self, target: &PoolTarget) -> Option<Arc<Mutex<()>>> {
        self.init_locks
            .lock()
            .expect("init_locks mutex poisoned")
            .get(target)
            .cloned()
    }

    #[cfg(test)]
    pub(super) fn holds_init_lock(&self, target: &PoolTarget) -> bool {
        self.init_locks
            .lock()
            .expect("init_locks mutex poisoned")
            .contains_key(target)
    }

    /// Returns the cached entry only if it matches `key` (i.e. mtimes
    /// haven't changed). A stale slot returns `None` — the caller will then
    /// rebuild and replace via [`Self::get_or_init`].
    fn lookup_fresh(&self, key: &PoolKey) -> Option<Arc<PoolEntry>> {
        let slots = self.slots.lock().expect("DuckDB pool slots mutex poisoned");
        slots
            .get(&key.target)
            .filter(|slot| slot.key == *key)
            .map(|slot| slot.entry.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_local_key(dir: &Path) -> PoolKey {
        // Build a synthetic key that doesn't require touching disk for the
        // file_signatures path. We construct it manually so unit tests stay
        // hermetic.
        PoolKey {
            target: PoolTarget::Local {
                dir: dir.to_path_buf(),
            },
            file_signatures: vec![(dir.to_path_buf(), 0, 0)],
            credential: None,
        }
    }

    fn dummy_entry() -> Result<(Connection, Vec<String>), OxyError> {
        let conn = Connection::open_in_memory()
            .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
        Ok((conn, vec![]))
    }

    /// MotherDuck opens a `duckdb_database` handle like any other DuckDB target,
    /// so it is subject to the same "one handle per database per process" rule
    /// that [`super::super::duckdb::checkout_file_connection`] documents. Pooling
    /// it means the second query reuses the first query's session instead of
    /// opening an independent handle to the same remote database.
    #[test]
    fn a_motherduck_target_is_opened_once_per_database() {
        let pool = DuckDBPool::default();
        let key = PoolKey::motherduck(Some("personal_data"), "token-a");

        let first = pool.get_or_init(key.clone(), dummy_entry).unwrap();
        let second = pool
            .get_or_init(key, || panic!("init must not re-run for a warm target"))
            .unwrap();

        assert!(
            Arc::ptr_eq(&first, &second),
            "the same md: database must share one pooled handle"
        );
    }

    /// Credentials are the MotherDuck analogue of a file mtime: the identity of
    /// the database is unchanged, but a handle opened under the old token must
    /// not be served after a rotation.
    #[test]
    fn rotating_the_motherduck_token_rebuilds_the_handle() {
        let pool = DuckDBPool::default();
        let old = pool
            .get_or_init(
                PoolKey::motherduck(Some("personal_data"), "token-a"),
                dummy_entry,
            )
            .unwrap();
        let weak_old = Arc::downgrade(&old);
        drop(old);

        let fresh = pool
            .get_or_init(
                PoolKey::motherduck(Some("personal_data"), "token-b"),
                dummy_entry,
            )
            .unwrap();

        assert!(
            weak_old.upgrade().is_none(),
            "a rotated token must evict the session opened under the old credential"
        );
        assert_eq!(
            pool.slots.lock().unwrap().len(),
            1,
            "rotation replaces the slot rather than accumulating one per token"
        );
        drop(fresh);
    }

    /// A `md:` primary is a network session the server can end under us. Without
    /// invalidation every later `try_clone()` hands out a connection on a dead
    /// handle and the connector stays broken until the process restarts — the
    /// per-query `Connection::open` this pool replaced was at least self-healing.
    #[test]
    fn invalidating_a_motherduck_target_forces_the_next_checkout_to_rebuild() {
        let pool = DuckDBPool::default();
        let key = PoolKey::motherduck(Some("personal_data"), "token-a");
        let target = PoolTarget::MotherDuck {
            database: Some("personal_data".to_string()),
        };

        let first = pool.get_or_init(key.clone(), dummy_entry).unwrap();
        let weak_first = Arc::downgrade(&first);
        drop(first);

        pool.invalidate(&target);

        assert!(
            weak_first.upgrade().is_none(),
            "invalidate must drop the cached session, not merely unlink it"
        );
        let mut rebuilt = false;
        let _second = pool
            .get_or_init(key, || {
                rebuilt = true;
                dummy_entry()
            })
            .unwrap();
        assert!(rebuilt, "the next checkout must reopen the session");
    }

    /// Pool keys live in a process-global map for the lifetime of the process, so
    /// the token itself must never be one — only a fingerprint of it.
    #[test]
    fn a_motherduck_key_does_not_retain_the_token() {
        let key = PoolKey::motherduck(Some("personal_data"), "super-secret-token");
        assert!(
            !format!("{key:?}").contains("super-secret-token"),
            "the pool key must fingerprint the token, never store it"
        );
    }

    #[test]
    fn replacing_a_stale_key_drops_the_old_entry() {
        let pool = DuckDBPool::default();
        let dir = PathBuf::from("/tmp/duckdb-pool-test");

        // First key: signatures = [(dir, 0, 0)]
        let key1 = fake_local_key(&dir);
        let entry1 = pool.get_or_init(key1.clone(), dummy_entry).unwrap();
        let weak1 = Arc::downgrade(&entry1);
        drop(entry1);

        // Second key: same target, different signatures (simulates an
        // mtime change). Inserting it should evict the first slot.
        let key2 = PoolKey {
            target: PoolTarget::Local { dir: dir.clone() },
            file_signatures: vec![(dir.clone(), 1, 0)],
            credential: None,
        };
        let entry2 = pool.get_or_init(key2, dummy_entry).unwrap();

        // The old PoolEntry is no longer reachable: only weak references
        // exist (ours) and we expect upgrade() to fail.
        assert!(
            weak1.upgrade().is_none(),
            "old PoolEntry should have been dropped on key replacement"
        );

        // Map size: one slot per target (not one per key generation).
        assert_eq!(
            pool.slots.lock().unwrap().len(),
            1,
            "pool must hold at most one slot per target"
        );

        drop(entry2);
    }

    #[test]
    fn matching_key_returns_cached_entry_without_rebuild() {
        let pool = DuckDBPool::default();
        let dir = PathBuf::from("/tmp/duckdb-pool-test-2");
        let key = fake_local_key(&dir);

        let entry1 = pool.get_or_init(key.clone(), dummy_entry).unwrap();
        let entry2 = pool
            .get_or_init(key.clone(), || {
                panic!("init must not run when the cached key matches")
            })
            .unwrap();

        assert!(
            Arc::ptr_eq(&entry1, &entry2),
            "matching key must return the same Arc<PoolEntry>"
        );
    }

    /// Verifies the per-target init lock: two threads that both miss the cache
    /// for the same `PoolTarget` are serialised — only ONE of them runs `init`;
    /// the second waits and then returns the entry the first inserted (the
    /// double-check in `get_or_init` hits the warm cache).  Both callers must
    /// end up with `Arc::ptr_eq` entries and the pool must hold exactly one slot.
    #[test]
    fn concurrent_init_for_same_target_leaves_one_slot() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::thread;

        let pool = Arc::new(DuckDBPool::default());
        let dir = PathBuf::from("/tmp/duckdb-pool-concurrent-test");
        // Count how many times `init` actually executes — with the per-target
        // lock it must be exactly 1 even when two threads race.
        let init_count = Arc::new(AtomicUsize::new(0));

        let pool1 = pool.clone();
        let dir1 = dir.clone();
        let count1 = init_count.clone();
        let t1 = thread::spawn(move || {
            let key = fake_local_key(&dir1);
            pool1.get_or_init(key, move || {
                count1.fetch_add(1, Ordering::SeqCst);
                dummy_entry()
            })
        });

        let pool2 = pool.clone();
        let dir2 = dir.clone();
        let count2 = init_count.clone();
        let t2 = thread::spawn(move || {
            let key = fake_local_key(&dir2);
            pool2.get_or_init(key, move || {
                count2.fetch_add(1, Ordering::SeqCst);
                dummy_entry()
            })
        });

        let entry1 = t1.join().expect("thread 1 panicked").unwrap();
        let entry2 = t2.join().expect("thread 2 panicked").unwrap();

        // Per-target init lock: exactly one init ran.
        assert_eq!(
            init_count.load(Ordering::SeqCst),
            1,
            "only one init must run when two threads race for the same target"
        );

        assert_eq!(
            pool.slots.lock().unwrap().len(),
            1,
            "pool must hold exactly one slot after a concurrent-init race"
        );

        // Both callers got the same entry (second caller hit the double-check).
        assert!(
            Arc::ptr_eq(&entry1, &entry2),
            "both callers must receive the same Arc<PoolEntry>"
        );

        entry1.checkout().expect("checkout failed");
    }
}
