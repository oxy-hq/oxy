//! Shared database handle for `oxy-app`'s in-source (`#[cfg(test)]`) tests.
//!
//! **The tests weren't running.** Three modules each hand-rolled a `test_db()`
//! gated on `OXY_TEST_DATABASE_URL` — a variable set nowhere in the repo: not in
//! `ci.yaml`, not in the `justfile`, not in any doc. CI sets `OXY_DATABASE_URL`.
//! So every one of those tests skipped, in CI and locally, forever, while
//! reading as a pass. That included the multi-tenant scoping test for workspace
//! health, which guards an invariant the product treats as correctness-critical.
//! A skip that is indistinguishable from a pass is worse than no test, because
//! it also stops anyone from noticing the gap.
//!
//! Skips only on an unset `OXY_DATABASE_URL` — a laptop with no database. A
//! connection or migration *failure* panics, because that's a broken test
//! environment, not an absent one.
//!
//! # Serializing migrations
//!
//! These callers are lib tests, and they migrate a **shared** database (unlike
//! the integration tests in `tests/`, which each create their own). Two things
//! follow, and both cut against the obvious approach:
//!
//! - **nextest runs every test in its own process.** So a process-local
//!   `OnceCell` around the migration dedupes within a single test and *nothing*
//!   across tests — six tests means six processes each migrating.
//! - **`serial-db` doesn't cover them.** The override in `.config/nextest.toml`
//!   is scoped `kind(test)` — integration binaries — so `kind(lib)` tests run in
//!   parallel with each other *and* alongside the serialized integration tests,
//!   which the group can't hold back on behalf of a non-member.
//!
//! Concurrent `Migrator::up` against one database is exactly the race that config
//! documents: `CREATE TABLE IF NOT EXISTS seaql_migrations_*` is not atomic
//! against a concurrent creator, and Postgres surfaces the collision as a
//! duplicate key on `pg_type_typname_nsp_index`.
//!
//! So migrations run under a Postgres **advisory lock**, which serializes across
//! processes no matter how many there are. The `OnceCell` stays as the
//! in-process fast path.
//!
//! # Serializing a test's own critical section
//!
//! The same problem, and the same fix, applies to any test that mutates rows
//! a concurrent `kind(lib)` test also touches — not just the migration
//! bootstrap below. `#[serial_test::serial]` does **not** substitute for
//! this: this crate's `Cargo.lock` pulls in `serial_test` with default
//! features only, so its `file_locks` feature (the cross-process lock,
//! backed by `fslock`) is off, and it falls back to an in-process
//! `parking_lot` mutex — which, given nextest's one-process-per-test
//! execution, never actually contends with anything. [`AdvisoryLock`] is the
//! real fix, reusable outside this module: acquire it with your own fixed
//! key (distinct from [`MIGRATION_LOCK_KEY`] and every other caller's) around
//! the critical section, exactly like the migration bootstrap does.

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection};
use tokio::sync::OnceCell;

/// Fast path only: skips re-migrating when one process asks twice. Correctness
/// across processes comes from [`MIGRATION_LOCK_KEY`], not from this.
static MIGRATED: OnceCell<()> = OnceCell::const_new();

/// Advisory-lock key for the migration critical section. Arbitrary but fixed —
/// keys share one namespace per database, so this only has to avoid colliding
/// with another advisory lock on the same DB, and nothing else in Oxy takes one.
const MIGRATION_LOCK_KEY: i64 = 0x0787_5EED;

/// A migrated connection to `OXY_DATABASE_URL`, or `None` when it's unset.
///
/// Callers early-return on `None`:
///
/// ```ignore
/// let Some(db) = test_db().await else { return };
/// ```
pub(crate) async fn test_db() -> Option<DatabaseConnection> {
    let url = std::env::var("OXY_DATABASE_URL").ok()?;
    let db = Database::connect(&url)
        .await
        .expect("OXY_DATABASE_URL is set but unreachable");

    MIGRATED
        .get_or_init(|| async {
            let lock = AdvisoryLock::acquire(&url, MIGRATION_LOCK_KEY).await;
            use migration::MigratorTrait;
            // Startup order, mirroring `cli::commands::admin`. The central
            // migrator alone is not enough: `agentic_runs` and friends belong to
            // the runtime migrator and its own tracking table.
            migration::Migrator::up(&db, None)
                .await
                .expect("central migrations");
            agentic_runtime::migration::RuntimeMigrator::up(&db, None)
                .await
                .expect("runtime migrations");
            lock.release().await;
        })
        .await;

    Some(db)
}

/// The URL [`test_db`] connects to, exposed for tests that need their own
/// out-of-band session — e.g. an [`AdvisoryLock`] around a critical section.
/// `None` under the same condition `test_db` skips on.
pub(crate) fn database_url() -> Option<String> {
    std::env::var("OXY_DATABASE_URL").ok()
}

/// A held `pg_advisory_lock`, on a connection of its own.
///
/// Advisory locks are per **session**, so a shared/pooled connection risks
/// the acquire and release landing on different sessions — this always opens
/// a dedicated single-connection handle for exactly that reason. Originally
/// built for the migration bootstrap below; reusable by any test that needs
/// to serialize its own critical section across nextest's per-process test
/// execution — see "Serializing a test's own critical section" at the top of
/// this file for why `#[serial_test::serial]` doesn't substitute for it here.
pub(crate) struct AdvisoryLock {
    conn: DatabaseConnection,
    key: i64,
}

impl AdvisoryLock {
    /// Blocks until every other session holding `key` has released it.
    pub(crate) async fn acquire(url: &str, key: i64) -> Self {
        // A pool of exactly one connection — see the struct doc for why.
        let mut opt = ConnectOptions::new(url.to_string());
        opt.max_connections(1).min_connections(1);
        let conn = Database::connect(opt)
            .await
            .expect("connect for advisory lock");
        conn.execute_unprepared(&format!("SELECT pg_advisory_lock({key})"))
            .await
            .expect("acquire advisory lock");
        Self { conn, key }
    }

    pub(crate) async fn release(self) {
        // Best-effort: a failure here is not worth failing a test over, because
        // dropping the connection ends the session and Postgres releases the
        // lock anyway. That's also what covers a panicking critical section —
        // the lock can't outlive the process that holds it, so a failed test
        // can't wedge the suite.
        let key = self.key;
        let _ = self
            .conn
            .execute_unprepared(&format!("SELECT pg_advisory_unlock({key})"))
            .await;
    }
}

/// Advisory-lock key for the singleton `airway_deployment_config` row.
///
/// Shared rather than declared twice on purpose: two test files write that row
/// — `server::api::admin::airway_config::deployment_tests` and
/// `airway_boot_tests` — and there is exactly **one** of it, so they must
/// contend on the *same* key. Two different keys would let them run
/// concurrently and overwrite each other's row rather than merely interleave,
/// which is the failure the lock exists to prevent. Distinct from
/// [`MIGRATION_LOCK_KEY`] and from every other caller's key, since keys share
/// one namespace per database. `AIRWDP` truncated: `41 49 52 57 44 50`.
pub(crate) const AIRWAY_DEPLOYMENT_LOCK_KEY: i64 = 0x4149_5257_4450;

/// Message to print when skipping, so a skipped run says which knob to turn.
pub(crate) const SKIP_MSG: &str = "skipping: OXY_DATABASE_URL not set";
