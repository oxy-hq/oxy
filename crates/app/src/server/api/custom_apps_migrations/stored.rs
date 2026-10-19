//! A build's declared migrations, re-read from the stored bundle.
//!
//! A publish has its bundle in hand and declares its migrations from it
//! ([`super::declare`], [`super::declare_airhouse`]). Every other move of
//! production's pointer — `oxyc promote`, `oxyc rollback`, the console's
//! *Make Live* and rollback, *promote latest* — has only the build row, and
//! still applies the build's pending files before the pointer moves
//! (environments design §5.5a, ruling 4). So the files are read back out of
//! the build store (`<dir>/*.sql` under the build's prefix) and declared by
//! the very same functions, against the manifest the build recorded: the
//! same files, the same checksums, the same rules.
//!
//! A build whose manifest declares no migrations reads nothing from the
//! store, so an app with no tables — and every build older than migrations —
//! pays no object-store round trip on a promote.

use std::collections::BTreeSet;

use entity::app_builds;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use super::apply::read_ledger;
use super::plan::{declare, declare_airhouse, plan};
use super::types::{
    DeclaredMigration, MigrationError, MigrationTarget, STORE_AIRHOUSE, STORE_OLTP,
};
use crate::server::api::custom_apps_build_store as store;
use crate::server::api::custom_apps_manifest::{airhouse_migrations_config, migrations_config};

/// What one stored build declares, per store.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StoredMigrations {
    pub oltp: Vec<DeclaredMigration>,
    pub airhouse: Vec<DeclaredMigration>,
}

impl StoredMigrations {
    /// The file names per store, `(oltp, airhouse)`.
    pub fn filenames(&self) -> (Vec<String>, Vec<String>) {
        let names = |m: &[DeclaredMigration]| m.iter().map(|f| f.filename.clone()).collect();
        (names(&self.oltp), names(&self.airhouse))
    }
}

/// The migrations `build` declares, read from its stored bundle. An `Err` is
/// the manifest's or the store's: a malformed block, a directory with no
/// `.sql` file, an edited file's bytes failing the Airhouse rules, or a store
/// that cannot be read (`Infra`, retryable).
pub async fn declared_in_build(
    build: &app_builds::Model,
    app_slug: &str,
) -> Result<StoredMigrations, MigrationError> {
    let manifest = build.manifest_json.as_ref();
    let dirs: BTreeSet<String> = [
        migrations_config(manifest).ok().flatten().map(|c| c.dir),
        airhouse_migrations_config(manifest)
            .ok()
            .flatten()
            .map(|c| c.dir),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut files = Vec::new();
    for dir in &dirs {
        let listed = store::files_under(build.app_id, &build.build_id, dir, ".sql")
            .await
            .map_err(|e| MigrationError::Infra {
                filename: String::new(),
                message: format!("the build's migrations could not be read back ({e})"),
            })?;
        files.extend(listed);
    }
    Ok(StoredMigrations {
        oltp: declare(manifest, &files)?,
        airhouse: declare_airhouse(manifest, &files, app_slug)?,
    })
}

/// The files of `declared` production's ledger has not recorded yet, per
/// store: what a production move would apply before the pointer moves. An
/// edited or renamed file is the same `Err` the apply would answer.
pub async fn pending_in_production(
    db: &DatabaseConnection,
    app_id: Uuid,
    declared: &StoredMigrations,
) -> Result<(Vec<String>, Vec<String>), MigrationError> {
    Ok((
        pending_in(db, app_id, STORE_OLTP, &declared.oltp).await?,
        pending_in(db, app_id, STORE_AIRHOUSE, &declared.airhouse).await?,
    ))
}

/// [`pending_in_production`] for one store. No ledger read for no files.
async fn pending_in(
    db: &DatabaseConnection,
    app_id: Uuid,
    store_name: &str,
    files: &[DeclaredMigration],
) -> Result<Vec<String>, MigrationError> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let ledger = read_ledger(db, app_id, store_name, &MigrationTarget::Production).await?;
    Ok(plan(files, &ledger)?
        .into_iter()
        .map(|m| m.filename.clone())
        .collect())
}

/// How many of `current`'s files `target` does not carry — what a rollback
/// warns about ("the target predates N applied migrations"). Never reversed:
/// the tables stay.
pub fn files_not_in(current: &StoredMigrations, target: &StoredMigrations) -> usize {
    let missing = |a: &[DeclaredMigration], b: &[DeclaredMigration]| {
        a.iter()
            .filter(|f| !b.iter().any(|g| g.filename == f.filename))
            .count()
    };
    missing(&current.oltp, &target.oltp) + missing(&current.airhouse, &target.airhouse)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> DeclaredMigration {
        DeclaredMigration {
            filename: name.to_string(),
            checksum: format!("sum-{name}"),
            sql: String::new(),
        }
    }

    #[test]
    fn a_rollback_counts_the_files_its_target_does_not_carry() {
        let current = StoredMigrations {
            oltp: vec![file("0001.sql"), file("0002.sql")],
            airhouse: vec![file("0001_visits.sql")],
        };
        let target = StoredMigrations {
            oltp: vec![file("0001.sql")],
            airhouse: Vec::new(),
        };
        assert_eq!(files_not_in(&current, &target), 2);
        assert_eq!(files_not_in(&target, &current), 0);
    }

    /// A build that declares nothing reads nothing: no manifest, no store.
    #[tokio::test]
    async fn a_build_with_no_migrations_block_reads_no_store() {
        // A build id the store refuses: had anything been read, this errs.
        let build: app_builds::Model = serde_json::from_value(serde_json::json!({
            "id": Uuid::nil(),
            "app_id": Uuid::nil(),
            "build_id": "../escape",
            "s3_prefix": "",
            "manifest_json": { "functions": {} },
            "created_at": chrono::Utc::now().fixed_offset(),
            "published_by": null,
            "published_via": null,
            "source_repo": null,
            "commit_sha": null,
            "source_branch": null,
            "validation_status": "passed",
            "validation_detail": null,
            "semantic_revision_id": null,
        }))
        .expect("an app_builds row");
        let declared = declared_in_build(&build, "store-ops")
            .await
            .expect("nothing to read");
        assert_eq!(declared, StoredMigrations::default());
    }
}
