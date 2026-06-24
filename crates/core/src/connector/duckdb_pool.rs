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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PoolKey {
    target: PoolTarget,
    /// Sorted `(path, mtime_secs, mtime_nanos)` for every file the handle
    /// depends on. `mtime` is captured as a tuple of `u64` + `u32` so the
    /// key is hashable / comparable (`SystemTime` is not).
    file_signatures: Vec<(PathBuf, u64, u32)>,
}

impl PoolKey {
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
        }
    }

    fn dummy_entry() -> Result<(Connection, Vec<String>), OxyError> {
        let conn = Connection::open_in_memory()
            .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
        Ok((conn, vec![]))
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

        // Exactly one slot in the map.
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
