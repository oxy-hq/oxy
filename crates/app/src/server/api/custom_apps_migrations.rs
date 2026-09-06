//! Schema migrations for a custom app: declared in the bundle, applied once,
//! recorded, and refused if edited.
//!
//! # The gap this closes
//!
//! `oxy publish` shipped code. Schema did not ship at all: an app's tables
//! arrived by a developer running a hand-maintained `.integrate.sh` that
//! `psql`'d **every** `schemas/*.sql` on **every** pass, with nothing recording
//! what had run. Measured on dev in `customer-apps/dev/delightree-demo`:
//!
//!  - Renaming a launcher-plan row *from the app* made a seed's
//!    `ON CONFLICT (template_id, phase, title)` stop matching, so the next pass
//!    **re-inserted the old row beside the new one** — 17 rows became 18.
//!  - A training-body upsert **restored its own text over an author's edit**, so
//!    the app's writes expired on the next pass.
//!
//! Both were patched with triggers inside the app's own schema. Those triggers
//! were a workaround for this module not existing.
//!
//! # The guarantee
//!
//! A migration runs **exactly once per app, ever**, recorded in
//! `custom_app_migrations`. Re-running is a no-op *by construction* — the
//! ledger is consulted, not the SQL's own defensiveness — so an author who
//! forgets `IF NOT EXISTS` gets the same answer as one who remembers.
//!
//! The load-bearing rule is [`plan`]: a file already in the ledger whose bytes
//! have **changed** is a hard error, not a silent re-run and not a silent skip.
//! That is what makes "edit a migration that already ran" impossible rather than
//! discouraged — an edit is invisible to a ledger keyed on filename alone, and
//! the divergence it creates (tenant A ran v1, tenant B runs v2) is exactly the
//! failure that is unrecoverable once noticed.
//!
//! # Boundaries
//!
//! - The schema is `app_<writer>` where the writer is **derived from the app's
//!   slug host-side** (`oxy_oltp::schema::app_writer_name`), never taken from
//!   the manifest — the same binding `ctx.oltp` uses. A manifest that could name
//!   its own schema is a manifest that can migrate another app's.
//! - SQL runs as the app's **own writer role**, not the tenant owner. The writer
//!   holds `CREATE` inside its one schema and nothing outside it, so containment
//!   is enforced by Postgres rather than by reviewing the file. (This differs
//!   from `oxy_oltp::migrator`, which applies a *workspace's* DDL as the owner
//!   because that DDL legitimately spans schemas.)
//! - Each file runs in its own transaction. A failure part-way leaves earlier
//!   files applied **and recorded**, which is what the ledger is for: fix the
//!   file, re-publish, and only the failed one is retried.
//!
//! # Known limitations
//!
//! The `.sql` files ride **inside the bundle**, so — like every other file in a
//! bundle — they are reachable over the app's own host
//! (`/customer-apps/<org>/<slug>/<dir>/0001.sql`). Nothing here redacts them.
//! A migration must therefore carry no credential and no data the app's own
//! viewers may not see; it is DDL and seed rows, not a secret.
//!
//! Only the **publish** path applies migrations, because that is where the
//! bundle is in hand. The admin console's *Make Live* and *roll back* repoint
//! the channel at a build already in the object store and run nothing. Rolling
//! back is unaffected (those files are in the ledger already); the live hole is
//! `--no-promote` followed by a console Make Live, which puts new code in front
//! of tables that were never created. Closing it needs the build store to list
//! and fetch a build's `<dir>/*.sql` — `custom_apps_build_store` has `get_object`
//! but no list — and then this module's [`apply_on_promote`] unchanged.
//!
//! `CREATE INDEX CONCURRENTLY` cannot run inside a transaction and is **not**
//! special-cased here (`oxy_oltp::migrator` is, via `mentions_concurrently`).
//! Such a file fails loudly with Postgres's own message rather than silently
//! losing its transaction — the safe direction. Lift it here if an app ever
//! needs one.

use std::collections::HashMap;

use chrono::Utc;
use entity::custom_app_migrations;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use sha2::{Digest, Sha256};
use tracing::{info, instrument, warn};
use uuid::Uuid;

use super::custom_apps_manifest::migrations_config;

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("{0}")]
    BadManifest(String),
    #[error(
        "the `migrations.dir` {0:?} in oxy-app.json is not a safe path inside the bundle — \
         use a plain relative directory such as \"migrations\""
    )]
    UnsafeDir(String),
    /// Declaring a directory the bundle does not populate is almost always a
    /// typo or a build that forgot to copy the files. Shipping the app anyway
    /// would land as `relation does not exist` on a user, so it is refused.
    #[error(
        "oxy-app.json declares migrations in {dir:?} but the bundle carries no `.sql` files \
         there — check the directory name and that your build copies it into the bundle"
    )]
    EmptyDir { dir: String },
    #[error("migration {filename:?} is not valid UTF-8 text: {message}")]
    NotUtf8 { filename: String, message: String },
    /// THE rule. See the module docs.
    #[error(
        "migration {filename:?} was already applied to this app with different contents \
         (recorded {applied}, bundle has {bundled}). Editing a migration that has already run \
         diverges this app's database from what the file says — add a NEW migration file \
         instead."
    )]
    ChecksumMismatch {
        filename: String,
        applied: String,
        bundled: String,
    },
    /// The same rule reached by the other route: an author who renames an
    /// applied file gets a new ledger key and would re-run it. That is precisely
    /// how the launcher-plan row got duplicated, so it is refused by content.
    #[error(
        "migration {filename:?} has the same contents as {applied_as:?}, which this app has \
         already applied — renaming or copying an applied migration would run it a second time. \
         Restore the original name, or write a new migration that makes the change you want."
    )]
    AlreadyAppliedUnderAnotherName {
        filename: String,
        applied_as: String,
    },
    #[error(
        "this app declares schema migrations but its OLTP schema could not be resolved: {0}. \
         Ask whoever operates this org to provision the app's OLTP store."
    )]
    NoSchema(String),
    #[error("could not connect to the app's OLTP database: {0}")]
    Connect(String),
    #[error(
        "another promote is applying this app's migrations — wait for it to finish and \
         re-publish"
    )]
    Busy,
    #[error("migration {filename:?} failed: {message}")]
    Failed { filename: String, message: String },
    /// The tenant connection or transaction machinery failed around a file —
    /// beginning a transaction, or committing one.
    ///
    /// Split from [`MigrationError::Failed`] because the two are opposite
    /// answers to "whose problem is this". `Failed` is the file's SQL being
    /// wrong: the author fixes it and re-publishes, and a retry without a change
    /// is pointless. This one is ours — a dropped connection, a tenant restart —
    /// and a retry is exactly the right response. Reporting it as author fault
    /// told CI "your change is wrong" about a blip, and reporting the reverse
    /// makes a permanently broken migration look like a flake worth retrying
    /// forever.
    #[error("the app store was unreachable while applying {filename}: {message}")]
    Infra { filename: String, message: String },
    #[error(
        "migration {filename:?} applied but could not be recorded in the ledger ({message}); \
         re-publishing will attempt it again, which will fail loudly rather than run it twice"
    )]
    LedgerWriteFailed { filename: String, message: String },
    #[error("database error: {0}")]
    Db(String),
}

impl MigrationError {
    /// Whether the publisher can fix this by changing the bundle.
    ///
    /// Drives the HTTP status: a 4xx tells CI "your change is wrong", a 5xx
    /// tells it "retry". Getting this backwards makes a permanently broken
    /// migration look like a flake worth retrying forever.
    pub fn is_author_fault(&self) -> bool {
        matches!(
            self,
            MigrationError::BadManifest(_)
                | MigrationError::UnsafeDir(_)
                | MigrationError::EmptyDir { .. }
                | MigrationError::NotUtf8 { .. }
                | MigrationError::ChecksumMismatch { .. }
                | MigrationError::AlreadyAppliedUnderAnotherName { .. }
                // `NoSchema` stays author fault: it is reached only when the
                // app's own SLUG cannot back a schema name, which is a bundle
                // fact the publisher controls. The store being unprovisioned is
                // `Infra`, deliberately absent from this list.
                | MigrationError::NoSchema(_)
                | MigrationError::Failed { .. }
        )
    }

    /// Whether re-running the same publish could succeed.
    ///
    /// `Infra` joins `Busy` here: a dropped connection or an unreachable store
    /// is transient by nature, and the same bundle republished after it clears
    /// will apply exactly the files it was going to apply.
    pub fn is_retryable(&self) -> bool {
        matches!(self, MigrationError::Busy | MigrationError::Infra { .. })
    }
}

/// One `.sql` file the bundle declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DeclaredMigration {
    /// Path RELATIVE to the declared directory — the ledger key. Relative so
    /// renaming the directory in `oxy-app.json` does not orphan the ledger and
    /// re-run every file against tables that already exist.
    pub filename: String,
    /// Lowercase hex SHA-256 of the bytes as shipped.
    pub checksum: String,
    pub sql: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Applied {
    pub applied: Vec<String>,
    pub already_applied: usize,
}

impl Applied {
    /// One line for the publish log. Empty when the app declares nothing, so a
    /// caller can log it unconditionally.
    pub(super) fn summary(&self) -> String {
        if self.applied.is_empty() && self.already_applied == 0 {
            return String::new();
        }
        format!(
            "{} schema migration(s) applied, {} already present",
            self.applied.len(),
            self.already_applied
        )
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Normalise and vet the declared directory.
///
/// The bundle's paths are already `..`-free (`unpack_tar_gz` rejects traversal),
/// so this guards the *manifest* side: `dir` is author-supplied and is used as a
/// prefix match, where `"/"` or `""` would sweep the entire bundle into the
/// migration set and `".."` would read as a path the author did not intend.
fn normalize_dir(dir: &str) -> Result<String, MigrationError> {
    let trimmed = dir.trim().trim_matches('/');
    if trimmed.is_empty()
        || trimmed
            .split('/')
            .any(|c| c == ".." || c == "." || c.is_empty())
        || dir.starts_with('/')
        || dir.contains('\\')
    {
        return Err(MigrationError::UnsafeDir(dir.to_string()));
    }
    Ok(trimmed.to_string())
}

/// Pull the bundle's `*.sql` files under `dir`, in **lexical filename order**.
///
/// Pure — no database, no manifest — so the ordering and the checksum are
/// testable without a tenant. Ordering is by the path relative to `dir`, which
/// for the ordinary flat directory is filename order; a nested layout sorts
/// deterministically too rather than by whatever order tar happened to emit.
pub(super) fn collect(
    files: &[(String, Vec<u8>)],
    dir: &str,
) -> Result<Vec<DeclaredMigration>, MigrationError> {
    let dir = normalize_dir(dir)?;
    let prefix = format!("{dir}/");
    let mut out = Vec::new();
    for (path, bytes) in files {
        let path = path.trim_start_matches("./");
        let Some(rel) = path.strip_prefix(&prefix) else {
            continue;
        };
        // Case-sensitive on purpose: object keys are, and accepting `.SQL`
        // here while the store treats it as a different file invites a bundle
        // that behaves differently on two machines.
        if !rel.ends_with(".sql") {
            continue;
        }
        let sql = String::from_utf8(bytes.clone()).map_err(|e| MigrationError::NotUtf8 {
            filename: rel.to_string(),
            message: e.to_string(),
        })?;
        out.push(DeclaredMigration {
            filename: rel.to_string(),
            checksum: sha256_hex(bytes),
            sql,
        });
    }
    if out.is_empty() {
        return Err(MigrationError::EmptyDir { dir });
    }
    out.sort_by(|a, b| a.filename.cmp(&b.filename));
    Ok(out)
}

/// Decide what still has to run, refusing anything that has already run.
///
/// `ledger` is `filename -> checksum` for THIS app. Returns the subset of
/// `declared` to apply, in the order given.
///
/// Three outcomes per file, and the two refusals are the point of the feature:
///
/// | ledger says | verdict |
/// | --- | --- |
/// | same name, same bytes | already applied — skip |
/// | same name, different bytes | [`MigrationError::ChecksumMismatch`] |
/// | different name, same bytes | [`MigrationError::AlreadyAppliedUnderAnotherName`] |
/// | nothing | apply |
///
/// A file in the ledger but absent from the bundle is **ignored**: it ran, and
/// deleting the file from the repo cannot un-run it.
pub(super) fn plan<'a>(
    declared: &'a [DeclaredMigration],
    ledger: &HashMap<String, String>,
) -> Result<Vec<&'a DeclaredMigration>, MigrationError> {
    // Reverse index for the rename rule. Built once; a ledger is tens of rows.
    let by_checksum: HashMap<&str, &str> = ledger
        .iter()
        .map(|(name, sum)| (sum.as_str(), name.as_str()))
        .collect();

    let mut pending = Vec::new();
    for m in declared {
        match ledger.get(&m.filename) {
            Some(applied) if applied == &m.checksum => continue,
            Some(applied) => {
                return Err(MigrationError::ChecksumMismatch {
                    filename: m.filename.clone(),
                    applied: applied.clone(),
                    bundled: m.checksum.clone(),
                });
            }
            None => {}
        }
        // Not in the ledger by name — but if its *contents* already ran under a
        // different name, applying it would run that SQL a second time. For a
        // seed-style `INSERT ... ON CONFLICT` that is silent duplication rather
        // than a loud `already exists`, which is the measured bug.
        if let Some(applied_as) = by_checksum.get(m.checksum.as_str()) {
            return Err(MigrationError::AlreadyAppliedUnderAnotherName {
                filename: m.filename.clone(),
                applied_as: applied_as.to_string(),
            });
        }
        pending.push(m);
    }
    Ok(pending)
}

/// What the bundle declares: the manifest block resolved and its `*.sql` files
/// pulled out, or an empty vec when the manifest declares nothing.
///
/// Split from [`apply_on_promote`] so `publish` can call it **before** the
/// bundle is uploaded and before any row is written. Two things fall out of
/// that: a malformed `migrations` block fails the publish without leaving an
/// orphan build behind, and a *draft* publish validates the block too — the
/// author finds out the directory name is wrong on the publish that carries it,
/// not on the promote weeks later.
///
/// It also keeps the SQL alive past the point where `publish` moves the whole
/// bundle into the object store: these files are kilobytes, so holding the
/// subset costs nothing.
pub(super) fn declare(
    manifest_json: Option<&serde_json::Value>,
    files: &[(String, Vec<u8>)],
) -> Result<Vec<DeclaredMigration>, MigrationError> {
    let Some(cfg) = migrations_config(manifest_json).map_err(MigrationError::BadManifest)? else {
        return Ok(Vec::new());
    };
    collect(files, &cfg.dir)
}

/// Read this app's ledger as `filename -> checksum`.
async fn read_ledger(
    db: &DatabaseConnection,
    app_id: Uuid,
) -> Result<HashMap<String, String>, MigrationError> {
    Ok(custom_app_migrations::Entity::find()
        .filter(custom_app_migrations::Column::AppId.eq(app_id))
        .all(db)
        .await
        .map_err(|e| MigrationError::Db(e.to_string()))?
        .into_iter()
        .map(|r| (r.filename, r.checksum))
        .collect())
}

/// A stable 64-bit advisory-lock key for one app.
///
/// Per-app rather than per-tenant: two apps in the same org have disjoint
/// schemas and disjoint ledgers, so serialising them against each other would
/// only make concurrent promotes slower.
fn app_lock_key(app_id: Uuid) -> i64 {
    let b = app_id.as_bytes();
    i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

/// Render a Postgres failure the way the author needs it — SQLSTATE, message,
/// detail, hint. `tokio_postgres::Error`'s own Display is just "db error".
fn pg_detail(e: &tokio_postgres::Error) -> String {
    match e.as_db_error() {
        Some(db) => {
            let mut msg = format!("[{}] {}", db.code().code(), db.message());
            if let Some(detail) = db.detail() {
                msg.push_str(&format!(" — {detail}"));
            }
            if let Some(hint) = db.hint() {
                msg.push_str(&format!(" (hint: {hint})"));
            }
            msg
        }
        None => e.to_string(),
    }
}

/// Apply this bundle's declared migrations to the app's own OLTP schema.
///
/// Called from `publish` **before** the published pointer moves, so a failure
/// leaves the app serving its previous build. A half-migrated app whose code
/// already shipped is worse than a promote that did not happen.
///
/// Returns `Applied::default()` for the common case: an app that declared
/// nothing (`declared` empty). That path costs one branch — no ledger read, no
/// writer resolution, no tenant connection — so an app with no tables pays
/// nothing for this feature existing.
#[instrument(skip(db, declared), fields(app_id = %app_id, app_slug = %app_slug))]
pub(super) async fn apply_on_promote(
    db: &DatabaseConnection,
    app_id: Uuid,
    app_slug: &str,
    org_id: Uuid,
    build_pk: Uuid,
    declared: &[DeclaredMigration],
) -> Result<Applied, MigrationError> {
    if declared.is_empty() {
        return Ok(Applied::default());
    }

    // Cheap pre-flight against the control plane. Two reasons it runs before any
    // tenant work: a promote that has nothing to apply must not open a tenant
    // connection at all, and an EDITED migration must fail the promote without
    // having touched the tenant database. The authoritative plan is recomputed
    // below under the lock.
    if plan(declared, &read_ledger(db, app_id).await?)?.is_empty() {
        return Ok(Applied {
            applied: Vec::new(),
            already_applied: declared.len(),
        });
    }

    // The writer is DERIVED from the slug, exactly as `ctx.oltp` derives it
    // (`custom_apps_functions/host.rs`). The manifest gets no say: it may
    // declare *that* there are migrations, never *where* they land.
    let writer_name = oxy_oltp::schema::app_writer_name(app_slug).ok_or_else(|| {
        MigrationError::NoSchema(format!(
            "the app's slug '{app_slug}' cannot back an OLTP schema (a slug must start with a \
             letter, be at most {max} characters, and use only lowercase letters, digits and \
             hyphens — a `_` is refused because it would collide with the hyphenated form)",
            max = oxy_oltp::schema::MAX_NAME_LEN,
        ))
    })?;
    let writer = oxy_oltp::schema::WriterRef::app(&writer_name)
        .map_err(|e| MigrationError::NoSchema(e.to_string()))?;
    // NOT `NoSchema`: an unprovisioned or disabled OLTP store is the OPERATOR's
    // state, and nothing the author can change in the bundle fixes it. Telling
    // CI "your change is wrong" about a store nobody has provisioned sends the
    // publisher to edit SQL that is already correct.
    let conn = oxy_oltp::resolver::resolve_writer_connection_for_org(db, org_id, &writer)
        .await
        .map_err(|e| MigrationError::Infra {
            filename: String::new(),
            message: format!("the app's OLTP store is not reachable: {e}"),
        })?;

    // `search_path` is already pinned to the writer's schema by the resolver, so
    // an unqualified `CREATE TABLE orders` lands in `app_<writer>` and a
    // reference to anything outside it fails on grants rather than on trust.
    let mut client = oxy_oltp::connect::connect(&conn.dsn, "custom app migration")
        .await
        .map_err(|e| MigrationError::Connect(e.to_string()))?;

    // Serialise promotes of the SAME app before re-reading the ledger. Without
    // this, two concurrent promotes both read "0002 unapplied" and both run its
    // DDL; the ledger's unique key would then reject only the second INSERT —
    // after the SQL had already run twice. `try`, not a blocking acquire: a
    // publish that hangs with no output is indistinguishable from a slow
    // migration, and one wedged session would block every later promote.
    let lock_key = app_lock_key(app_id);
    let got: bool = client
        .query_one("SELECT pg_try_advisory_lock($1)", &[&lock_key])
        .await
        .map_err(|e| MigrationError::Connect(format!("acquire apply lock: {e}")))?
        .get(0);
    if !got {
        return Err(MigrationError::Busy);
    }

    // Re-read under the lock. The pre-flight above is an optimisation and a
    // fast refusal; THIS is the plan that runs.
    let ledger = read_ledger(db, app_id).await?;
    let pending = plan(declared, &ledger)?;
    let mut outcome = Applied {
        applied: Vec::new(),
        already_applied: declared.len() - pending.len(),
    };

    info!(
        schema = %conn.schema,
        pending = pending.len(),
        "applying custom-app schema migrations"
    );

    for m in pending {
        let txn = client
            .transaction()
            .await
            .map_err(|e| MigrationError::Infra {
                filename: m.filename.clone(),
                message: pg_detail(&e),
            })?;
        // `batch_execute` (simple query protocol) so a file may hold several
        // statements. All of them commit together or none do, which is what
        // makes a failed migration leave nothing behind for the next promote to
        // trip over.
        txn.batch_execute(&m.sql)
            .await
            .map_err(|e| MigrationError::Failed {
                filename: m.filename.clone(),
                message: pg_detail(&e),
            })?;
        txn.commit().await.map_err(|e| MigrationError::Infra {
            filename: m.filename.clone(),
            message: format!("committing: {e}"),
        })?;

        // The ledger lives in the CONTROL database and the DDL just committed in
        // the TENANT database, so these two cannot be one transaction. The
        // ordering is chosen for which failure is louder: recording second means
        // a crash in this window re-attempts the file on the next promote, where
        // non-idempotent DDL fails with Postgres's own `already exists`. The
        // other order would record a file that never ran and silently skip it
        // forever. Loud and recoverable beats silent and wrong.
        custom_app_migrations::ActiveModel {
            app_id: Set(app_id),
            filename: Set(m.filename.clone()),
            checksum: Set(m.checksum.clone()),
            applied_at: Set(Utc::now().fixed_offset()),
            applied_by_build: Set(Some(build_pk)),
        }
        .insert(db)
        .await
        .map_err(|e| {
            warn!(filename = %m.filename, error = %e, "migration applied but not recorded");
            MigrationError::LedgerWriteFailed {
                filename: m.filename.clone(),
                message: e.to_string(),
            }
        })?;

        info!(filename = %m.filename, schema = %conn.schema, "applied custom-app migration");
        outcome.applied.push(m.filename.clone());
    }

    // Best-effort: the lock is session-scoped and the client is dropped on
    // return anyway.
    let _ = client
        .execute("SELECT pg_advisory_unlock($1)", &[&lock_key])
        .await;

    Ok(outcome)
}

#[cfg(test)]
mod collect_tests {
    use super::*;

    fn bundle(entries: &[(&str, &str)]) -> Vec<(String, Vec<u8>)> {
        entries
            .iter()
            .map(|(p, c)| (p.to_string(), c.as_bytes().to_vec()))
            .collect()
    }

    /// Lexical order, and only `.sql` under the declared directory.
    #[test]
    fn collects_sql_under_the_dir_in_lexical_order() {
        let files = bundle(&[
            ("index.html", "<html>"),
            ("migrations/0002_seed.sql", "INSERT INTO t VALUES (1);"),
            ("migrations/0001_init.sql", "CREATE TABLE t (id int);"),
            ("migrations/README.md", "not sql"),
            ("other/0003_nope.sql", "CREATE TABLE u (id int);"),
        ]);
        let got = collect(&files, "migrations").expect("collect");
        assert_eq!(
            got.iter().map(|m| m.filename.as_str()).collect::<Vec<_>>(),
            vec!["0001_init.sql", "0002_seed.sql"],
            "tar order must not decide apply order"
        );
    }

    /// The ledger key is relative to the directory, so moving the directory
    /// does not re-run everything against tables that already exist.
    #[test]
    fn the_ledger_key_is_relative_to_the_declared_dir() {
        let files = bundle(&[("db/sql/0001_init.sql", "CREATE TABLE t (id int);")]);
        let got = collect(&files, "db/sql").expect("collect");
        assert_eq!(got[0].filename, "0001_init.sql");
    }

    /// Same bytes → same checksum, and it is the bytes that are hashed, not the
    /// name. Both halves matter: the first is what makes a re-publish a no-op,
    /// the second is what catches an edit.
    #[test]
    fn the_checksum_is_over_the_bytes() {
        let a = collect(&bundle(&[("m/0001.sql", "SELECT 1;")]), "m").unwrap();
        let b = collect(&bundle(&[("m/0009_renamed.sql", "SELECT 1;")]), "m").unwrap();
        let c = collect(&bundle(&[("m/0001.sql", "SELECT 2;")]), "m").unwrap();
        assert_eq!(a[0].checksum, b[0].checksum, "the name must not be hashed");
        assert_ne!(a[0].checksum, c[0].checksum, "the bytes must be hashed");
        assert_eq!(a[0].checksum.len(), 64, "lowercase hex sha256");
    }

    /// Declaring a directory the bundle does not populate is a typo or a build
    /// that forgot to copy it. Silently shipping no schema is the exact failure
    /// this module exists to prevent.
    #[test]
    fn a_declared_dir_with_no_sql_is_an_error_not_an_empty_plan() {
        let files = bundle(&[("index.html", "<html>"), ("migrations/README.md", "hi")]);
        let e = collect(&files, "migrations").expect_err("must refuse");
        assert!(matches!(e, MigrationError::EmptyDir { .. }), "got {e:?}");
        assert!(e.to_string().contains("migrations"), "names the dir");
    }

    /// A `dir` that would sweep the whole bundle, or escape it, is refused
    /// rather than normalised into something plausible.
    #[test]
    fn an_unsafe_dir_is_refused() {
        let files = bundle(&[("m/0001.sql", "SELECT 1;")]);
        for dir in ["", "/", "..", "../m", "/m", "m/../..", "m\\n"] {
            let e = collect(&files, dir).expect_err(&format!("must refuse {dir:?}"));
            assert!(matches!(e, MigrationError::UnsafeDir(_)), "{dir:?}: {e:?}");
        }
    }
}

#[cfg(test)]
mod plan_tests {
    use super::*;

    fn declared(entries: &[(&str, &str)]) -> Vec<DeclaredMigration> {
        entries
            .iter()
            .map(|(name, sql)| DeclaredMigration {
                filename: name.to_string(),
                checksum: sha256_hex(sql.as_bytes()),
                sql: sql.to_string(),
            })
            .collect()
    }

    fn ledger(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries
            .iter()
            .map(|(name, sql)| (name.to_string(), sha256_hex(sql.as_bytes())))
            .collect()
    }

    #[test]
    fn an_empty_ledger_applies_everything_in_order() {
        let d = declared(&[("0001.sql", "a"), ("0002.sql", "b")]);
        let pending = plan(&d, &HashMap::new()).expect("plan");
        assert_eq!(
            pending.iter().map(|m| &m.filename).collect::<Vec<_>>(),
            vec!["0001.sql", "0002.sql"]
        );
    }

    /// The whole point: a second promote of an unchanged bundle runs nothing.
    /// Not because the SQL is defensive — because the ledger says so.
    #[test]
    fn a_re_promote_of_the_same_bundle_is_a_no_op() {
        let d = declared(&[("0001.sql", "a"), ("0002.sql", "b")]);
        let l = ledger(&[("0001.sql", "a"), ("0002.sql", "b")]);
        assert!(plan(&d, &l).expect("plan").is_empty());
    }

    #[test]
    fn only_the_new_file_runs() {
        let d = declared(&[("0001.sql", "a"), ("0002.sql", "b")]);
        let l = ledger(&[("0001.sql", "a")]);
        let pending = plan(&d, &l).expect("plan");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].filename, "0002.sql");
    }

    /// THE rule. A file already in the ledger whose bytes changed must fail the
    /// promote and NAME the file — not re-run (divergence) and not skip
    /// (a change the author believes shipped and did not).
    ///
    /// Mutation-tested: flipping `plan`'s checksum comparison to always-equal
    /// turns this into a silent skip and this test fails on `expect_err`.
    #[test]
    fn an_edited_applied_migration_is_a_hard_error_naming_the_file() {
        let d = declared(&[("0001_orders.sql", "CREATE TABLE orders (id int);")]);
        let l = ledger(&[("0001_orders.sql", "CREATE TABLE orders (id bigint);")]);

        let e = plan(&d, &l).expect_err("an edited migration must fail the promote");
        assert!(
            matches!(e, MigrationError::ChecksumMismatch { .. }),
            "got {e:?}"
        );
        let msg = e.to_string();
        assert!(msg.contains("0001_orders.sql"), "must name the file: {msg}");
        // The message has to say what to do instead, or the author's next move
        // is to edit the file again.
        assert!(msg.contains("add a NEW migration file"), "got {msg}");
    }

    /// The same rule reached by renaming rather than editing — the shape that
    /// actually bit us, because a seed's `ON CONFLICT` re-inserts silently
    /// instead of failing with `already exists`.
    #[test]
    fn re_running_applied_sql_under_a_new_name_is_refused() {
        let sql = "INSERT INTO launcher_plan (title) VALUES ('Week 1') ON CONFLICT DO NOTHING;";
        let d = declared(&[("0007_relaunch_plan.sql", sql)]);
        let l = ledger(&[("0002_plan.sql", sql)]);

        let e = plan(&d, &l).expect_err("a renamed applied migration must fail");
        assert!(
            matches!(e, MigrationError::AlreadyAppliedUnderAnotherName { .. }),
            "got {e:?}"
        );
        let msg = e.to_string();
        assert!(msg.contains("0007_relaunch_plan.sql"), "got {msg}");
        assert!(
            msg.contains("0002_plan.sql"),
            "names what it duplicates: {msg}"
        );
    }

    /// A migration deleted from the repo stays applied. The ledger records what
    /// ran; editing the repo cannot un-run it, and re-running the survivors
    /// because a sibling vanished would be gratuitous.
    #[test]
    fn a_file_in_the_ledger_but_not_the_bundle_is_ignored() {
        let d = declared(&[("0002.sql", "b")]);
        let l = ledger(&[("0001_deleted.sql", "a"), ("0002.sql", "b")]);
        assert!(plan(&d, &l).expect("plan").is_empty());
    }

    /// A mismatch must be refused before ANY file runs, including files that
    /// sort before it. Applying 0001 and then refusing 0002 would leave the
    /// promote failed with the tenant already changed.
    #[test]
    fn a_mismatch_refuses_the_whole_plan_not_just_that_file() {
        let d = declared(&[
            ("0001.sql", "a"),
            ("0002.sql", "changed"),
            ("0003.sql", "c"),
        ]);
        let l = ledger(&[("0002.sql", "original")]);
        assert!(plan(&d, &l).is_err(), "0001 must not be planned");
    }

    #[test]
    fn error_classification_drives_the_right_status() {
        assert!(
            MigrationError::ChecksumMismatch {
                filename: "x".into(),
                applied: "a".into(),
                bundled: "b".into(),
            }
            .is_author_fault(),
            "an edited migration is a 4xx — retrying it forever fixes nothing"
        );
        assert!(
            !MigrationError::Busy.is_author_fault() && MigrationError::Busy.is_retryable(),
            "a contended promote is the one case worth retrying"
        );
        assert!(
            !MigrationError::Db("pool exhausted".into()).is_author_fault(),
            "our failure must not be reported as the author's"
        );
    }
}

#[cfg(test)]
mod manifest_tests {
    use super::*;

    /// The common case: an app with no tables declares nothing and pays
    /// nothing. If this ever became an error, every existing app would stop
    /// publishing.
    #[test]
    fn a_manifest_with_no_block_declares_no_migrations() {
        let m = serde_json::json!({ "schemaVersion": 2, "slug": "demo" });
        assert!(migrations_config(Some(&m)).expect("parse").is_none());
        assert!(migrations_config(None).expect("parse").is_none());
    }

    #[test]
    fn a_declared_dir_is_read() {
        let m = serde_json::json!({
            "schemaVersion": 2,
            "slug": "demo",
            "migrations": { "dir": "migrations" }
        });
        let cfg = migrations_config(Some(&m))
            .expect("parse")
            .expect("present");
        assert_eq!(cfg.dir, "migrations");
    }

    /// A present-but-broken block must NOT read as "no migrations" — that
    /// ships code without its tables, which is the whole bug. Unlike the
    /// retention block, whose parse failure safely means "nothing expires".
    #[test]
    fn a_malformed_block_is_an_error_not_an_absence() {
        for bad in [
            serde_json::json!({ "migrations": {} }),
            serde_json::json!({ "migrations": "migrations" }),
            serde_json::json!({ "migrations": { "directory": "migrations" } }),
            serde_json::json!({ "migrations": [] }),
        ] {
            let e = migrations_config(Some(&bad)).expect_err(&format!("must refuse {bad}"));
            assert!(e.contains("dir"), "must say what it wanted: {e}");
        }
    }

    /// An explicit `null` is a generator emitting an unset optional, not a
    /// malformed block.
    #[test]
    fn an_explicit_null_block_declares_nothing() {
        let m = serde_json::json!({ "migrations": serde_json::Value::Null });
        assert!(migrations_config(Some(&m)).expect("parse").is_none());
    }
}
