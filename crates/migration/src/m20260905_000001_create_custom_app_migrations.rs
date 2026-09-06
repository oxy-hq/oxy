use sea_orm_migration::prelude::*;

/// The per-app schema-migration ledger: which `.sql` files a custom app has
/// already applied to its own `app_<writer>` OLTP schema, and with what content.
///
/// # Why this table exists
///
/// `oxy publish` shipped code and nothing else. An app's tables arrived by a
/// developer running a hand-maintained `.integrate.sh` that `psql`'d **every**
/// `schemas/*.sql` on **every** pass, with nothing recording what had run. Two
/// measured consequences on dev (`customer-apps/dev/delightree-demo`):
///
///  - A seed's `ON CONFLICT (template_id, phase, title)` stopped matching once
///    a row was renamed *from the app*, so the re-run **re-inserted the old row
///    beside the new one** — 17 launcher-plan rows became 18.
///  - A training-body upsert **restored its own text over an author's edit**, so
///    the app's writes expired on the next pass.
///
/// Both were patched with triggers inside the app's own schema — a workaround
/// for the platform not having this table. With it, a migration runs exactly
/// once per app, ever, and a re-run is a no-op *by construction* rather than
/// because every author remembered `IF NOT EXISTS`.
///
/// # Column notes
///
/// - **`(app_id, filename)` is the primary key**, which is the uniqueness the
///   whole feature rests on. `filename` is the path *relative to the declared
///   migrations directory*, so renaming the directory in `oxy-app.json` does not
///   orphan a ledger and re-run every file.
/// - **`checksum`** is what turns "already applied" into a *decision* rather
///   than an assumption. A file whose recorded checksum differs from the one in
///   the bundle is a hard error at promote — see
///   `custom_apps_migrations::plan`.
/// - **`applied_by_build` is `ON DELETE SET NULL`, never `CASCADE`.**
///   `gc_builds` reaps build rows beyond `KEEP_BUILDS` after every publish, so a
///   cascade here would quietly empty the ledger of any app that has published
///   ten times — and an empty ledger means every migration runs again. The
///   provenance is worth keeping but is not worth the ledger row.
/// - `app_id` **does** cascade: deleting an app deprovisions its OLTP writer and
///   drops `app_<writer>` with it, so the ledger describes a schema that no
///   longer exists.
///
/// This is the CONTROL-plane ledger, deliberately separate from
/// `oxy_oltp::platform::MIGRATIONS_TABLE` (which lives inside the tenant
/// database and records a *workspace's* `schemas/*.sql`). The two never key on
/// the same thing: that one is per-tenant and revision-scoped, this one is
/// per-app.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE IF NOT EXISTS custom_app_migrations (
                app_id            UUID NOT NULL
                    REFERENCES apps(id) ON DELETE CASCADE,
                -- Path relative to the bundle's declared migrations directory.
                filename          TEXT NOT NULL,
                -- Lowercase hex SHA-256 of the file's bytes as shipped.
                checksum          TEXT NOT NULL,
                applied_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
                -- Provenance only. NULLed rather than cascaded when the build is
                -- GC'd, so the ledger outlives the bundle that carried the file.
                applied_by_build  UUID
                    REFERENCES app_builds(id) ON DELETE SET NULL,
                PRIMARY KEY (app_id, filename)
            );
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS custom_app_migrations CASCADE")
            .await?;
        Ok(())
    }
}
