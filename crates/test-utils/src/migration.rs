//! Shared-database migration ordering for integration test fixtures.
//!
//! `agentic-pipeline`, `agentic-runtime`, and `agentic-airway` integration
//! tests all run against **one** Postgres database: CI points every test
//! binary's `OXY_DATABASE_URL` at the same instance, and locally the
//! testcontainer fixtures reuse a single container via
//! `ReuseDirective::Always`. Each test binary is its own process, and
//! nextest gives no ordering guarantee between binaries — so whichever
//! fixture's `test_db()` reaches the shared database first is the one that
//! actually creates the schema for everyone else.
//!
//! # Why central must run first
//!
//! Production always runs the central migrator (`crates/migration`) before
//! the agentic-runtime migrator (see `cli/commands/serve.rs`, `airway.rs`,
//! `admin.rs`), so `agentic_runtime::migration` is written defensively to
//! tolerate running *second*. The clearest evidence is in
//! `RationalizeStatusModel` (`crates/agentic/runtime/src/migration.rs`):
//!
//! ```text
//! // Ensure thread_id column exists (may have been added by central migrator
//! // or may be missing in test databases that only run runtime migrations).
//! if !column_exists(manager, "agentic_runs", "thread_id").await? {
//!     db.execute_unprepared("ALTER TABLE agentic_runs ADD COLUMN thread_id UUID").await?;
//! }
//! ```
//!
//! Central's equivalent migrations (`m20260318_000001_add_thread_id_to_agentic_runs`,
//! `m20260401_000001_add_spec_hint_to_agentic_runs`,
//! `m20260402_000001_add_thinking_mode_to_agentic_runs`) do **not** carry that
//! guard, because production never needs it — central always leads. If a test
//! fixture runs `RuntimeMigrator::up` alone and happens to reach the shared DB
//! first, central then arrives second and fails outright: `42701 column
//! "thread_id" of relation "agentic_runs" already exists`, or `42P07` on an
//! index central creates without `.if_not_exists()`. This is exactly the
//! non-determinism this helper exists to remove.
//!
//! # The fix, and why the migrators are injected
//!
//! Every fixture on the shared database migrates in the same order production
//! does: central, then runtime, then (optionally) a domain migrator. Whichever
//! binary gets there first leaves the DB correctly migrated; every other binary
//! finds everything already applied and its migration calls no-op through.
//!
//! **The order is carried by the types, not by the caller.** [`migrate_shared_test_db`]
//! hard-codes central and takes the runtime migrator as a type parameter; it
//! returns a [`DomainMigrations`] token, and a domain migrator can *only* be
//! run through that token. There is no way to obtain one without having already
//! run central-then-runtime, so a fixture cannot get the order wrong by
//! accident — which is the property, and the reason this lives in one helper
//! instead of three copy-pasted lines per fixture:
//!
//! ```ignore
//! oxy_test_utils::migration::migrate_shared_test_db::<RuntimeMigrator>(&db)
//!     .await
//!     .expect("shared migrations")
//!     .then::<AirwayMigrator>()
//!     .await
//!     .expect("airway migrations");
//! ```
//!
//! Injecting the migrators rather than naming them is also what keeps this
//! crate's dependency direction right. `oxy-test-utils` is a shared *platform*
//! test crate — `oxy` and `oxy-app` dev-depend on it too — and platform crates
//! must never depend on an `agentic-*` crate (`internal-docs/backend-architecture.md`,
//! "Platform → Agentic: NEVER"). Naming `RuntimeMigrator` here would have put
//! the whole agentic tree behind every `cargo nextest run -p oxy` and made this
//! crate unusable anywhere that doesn't want it. Generic parameters cost
//! nothing at the call site — every fixture that passes `RuntimeMigrator`
//! already depends on `agentic-runtime` — and cost this crate no dependency at
//! all.

use sea_orm::{DatabaseConnection, DbErr};
use sea_orm_migration::MigratorTrait;

/// Runs the central migrator, then `Runtime`, against `db` — the production
/// order every fixture sharing the test database must replicate. See the module
/// docs for why the order is load-bearing.
///
/// `Runtime` is `agentic_runtime::migration::RuntimeMigrator` at every current
/// call site; it is a parameter so this crate needs no agentic dependency.
///
/// Idempotent: safe to call from every test binary's `test_db()`, even though
/// only one of them will actually apply anything on a given shared database.
///
/// Returns the [`DomainMigrations`] token — the only way to run a domain
/// migrator in the right place.
pub async fn migrate_shared_test_db<Runtime: MigratorTrait>(
    db: &DatabaseConnection,
) -> Result<DomainMigrations<'_>, DbErr> {
    migration::Migrator::up(db, None).await?;
    Runtime::up(db, None).await?;
    Ok(DomainMigrations(db))
}

/// Proof that central and runtime have already been applied to the wrapped
/// connection. Only [`migrate_shared_test_db`] can produce one.
///
/// A fixture with no domain migrator simply drops it.
pub struct DomainMigrations<'a>(&'a DatabaseConnection);

impl<'a> DomainMigrations<'a> {
    /// Runs one domain migrator (`AirwayMigrator`, `AutomationMigrator`,
    /// `AnalyticsMigrator`, …) after central and runtime.
    ///
    /// Chainable, for the rare fixture that needs two. Domain migrators own
    /// disjoint tables behind their own tracking tables, so their order
    /// relative to *each other* is not load-bearing — only their position
    /// after runtime is, and that is what holding this token proves.
    pub async fn then<Domain: MigratorTrait>(self) -> Result<Self, DbErr> {
        Domain::up(self.0, None).await?;
        Ok(self)
    }
}
