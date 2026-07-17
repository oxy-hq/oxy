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
            let lock = MigrationLock::acquire(&url).await;
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

/// A held `pg_advisory_lock`, on a connection of its own.
struct MigrationLock(DatabaseConnection);

impl MigrationLock {
    /// Blocks until every other test process has finished migrating.
    async fn acquire(url: &str) -> Self {
        // A pool of exactly one connection. Advisory locks are per *session*, so
        // on a multi-connection pool the unlock could land on a different
        // connection than the lock — leaving the lock held until that session
        // closed, and every later process blocked behind it.
        let mut opt = ConnectOptions::new(url.to_string());
        opt.max_connections(1).min_connections(1);
        let conn = Database::connect(opt)
            .await
            .expect("connect for migration lock");
        conn.execute_unprepared(&format!("SELECT pg_advisory_lock({MIGRATION_LOCK_KEY})"))
            .await
            .expect("acquire migration lock");
        Self(conn)
    }

    async fn release(self) {
        // Best-effort: a failure here is not worth failing a test over, because
        // dropping the connection ends the session and Postgres releases the
        // lock anyway. That's also what covers a panicking migration — the lock
        // can't outlive the process that holds it, so a failed test can't wedge
        // the suite.
        let _ = self
            .0
            .execute_unprepared(&format!("SELECT pg_advisory_unlock({MIGRATION_LOCK_KEY})"))
            .await;
    }
}

/// Message to print when skipping, so a skipped run says which knob to turn.
pub(crate) const SKIP_MSG: &str = "skipping: OXY_DATABASE_URL not set";
