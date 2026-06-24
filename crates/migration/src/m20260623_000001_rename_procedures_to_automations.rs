//! Rename the "procedure" tables to "automation".
//!
//! Part of the product-wide Procedures/Workflows → **Automations** rename.
//! The physical tables are renamed to the new canonical term, and a
//! back-compat VIEW under the OLD name is left in place so any external
//! consumer (raw SQL, BI tools, ad-hoc queries) that still references
//! `procedure_definitions` / `customer_app_procedure_runs` keeps working
//! during the deprecation window.
//!
//! Renamed tables:
//!   - `procedure_definitions`     → `automation_definitions`
//!   - `customer_app_procedure_runs` → `customer_app_automation_runs`
//!
//! Notes:
//!   - `ALTER TABLE ... RENAME` carries indexes, constraints, and the
//!     primary key along automatically, so no index re-creation is needed.
//!   - All statements are guarded (`IF EXISTS` / `IF NOT EXISTS`) so a
//!     partially-applied dev DB replays cleanly.
//!   - The compatibility views are read-only by default; writers in the
//!     codebase target the renamed base tables via the SeaORM entities,
//!     which now carry `table_name = "automation_definitions"` etc.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const RENAMES: &[(&str, &str)] = &[
    ("procedure_definitions", "automation_definitions"),
    (
        "customer_app_procedure_runs",
        "customer_app_automation_runs",
    ),
];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for (old, new) in RENAMES {
            // Rename the base table only when it still exists under the old
            // name AND the new name is not already present (idempotent replay).
            db.execute_unprepared(&format!(
                "DO $$ BEGIN \
                   IF EXISTS (SELECT 1 FROM information_schema.tables \
                              WHERE table_name = '{old}' AND table_type = 'BASE TABLE') \
                      AND NOT EXISTS (SELECT 1 FROM information_schema.tables \
                              WHERE table_name = '{new}' AND table_type = 'BASE TABLE') \
                   THEN \
                     ALTER TABLE {old} RENAME TO {new}; \
                   END IF; \
                 END $$;"
            ))
            .await?;

            // Back-compat read view under the old name. Dropped-and-created so
            // it always reflects the renamed base table's current columns.
            //
            // Guarded on the new name being a BASE TABLE and the old name NOT
            // being one: if the old name still exists as a base table (e.g.
            // both base tables coexist and the rename above was skipped), a
            // bare `DROP VIEW IF EXISTS {old}` would raise "{old} is not a
            // view" — `IF EXISTS` only suppresses the missing-relation case,
            // not the wrong-relkind case. Skipping the view step in that
            // edge keeps a partially-applied dev DB replaying cleanly.
            db.execute_unprepared(&format!(
                "DO $$ BEGIN \
                   IF EXISTS (SELECT 1 FROM information_schema.tables \
                              WHERE table_name = '{new}' AND table_type = 'BASE TABLE') \
                      AND NOT EXISTS (SELECT 1 FROM information_schema.tables \
                              WHERE table_name = '{old}' AND table_type = 'BASE TABLE') \
                   THEN \
                     DROP VIEW IF EXISTS {old}; \
                     CREATE VIEW {old} AS SELECT * FROM {new}; \
                   END IF; \
                 END $$;"
            ))
            .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for (old, new) in RENAMES {
            // Drop the compat view first so the name is free for the table.
            db.execute_unprepared(&format!("DROP VIEW IF EXISTS {old};"))
                .await?;
            db.execute_unprepared(&format!(
                "DO $$ BEGIN \
                   IF EXISTS (SELECT 1 FROM information_schema.tables \
                              WHERE table_name = '{new}' AND table_type = 'BASE TABLE') \
                      AND NOT EXISTS (SELECT 1 FROM information_schema.tables \
                              WHERE table_name = '{old}' AND table_type = 'BASE TABLE') \
                   THEN \
                     ALTER TABLE {new} RENAME TO {old}; \
                   END IF; \
                 END $$;"
            ))
            .await?;
        }
        Ok(())
    }
}
