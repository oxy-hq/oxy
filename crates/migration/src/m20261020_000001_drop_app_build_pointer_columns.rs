//! Drops `apps.draft_build_id` and `apps.published_build_id` (environments
//! Phase 1c, `internal-docs/2026-09-10-custom-app-environments-design.md` §7;
//! `internal-docs/custom-apps-architecture.md` §5, W2.5).
//!
//! From this migration on, `app_environments.build_id` is the only store of the
//! build an environment serves: the `production` row is what `published_build_id`
//! was, the `staging` row what `draft_build_id` was. Neither column had an index
//! or a foreign key, so there is nothing else to drop.
//!
//! **Before dropping,** it gives every app that still names a build in one of the
//! columns and has no row for that environment the row, so the pointer is not
//! lost. Phase 1a's backfill and mirrored writes should have left none; this makes
//! "none" a fact instead of an assumption. A pointer at a build row that no longer
//! exists is skipped: it would fail the row's foreign key, and it served nothing
//! (`custom_apps_serve` answers 404 for it). Rows that exist are never touched, so
//! a disagreement between a row and its column resolves to the row, which is what
//! every reader has read since Phase 1b.
//!
//! **Third step of expand/contract, one release after the second.** The release
//! before the one carrying this migration neither reads nor writes the columns
//! (the write-only mirror was removed in the step before), so dropping them breaks
//! no replica of it. Do not promote this migration in the same release that
//! removes the mirror, or before that release is what prod runs: the release
//! before *that* still reads and writes both columns. `down` re-adds the columns
//! from the rows, which restores the data for a manual rollback.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Rows for pointers that have none, and the `backfill` events that make the
/// build they name promotable, exactly as `m20260922_000001_app_environments`
/// wrote them: promotion asks the events (`staging_never_served`), not the rows,
/// and every build a pointer names counts as served by staging. Events are
/// written only for the rows this run creates (`RETURNING`), so a row that
/// exists keeps its own history and a second run adds nothing. Public so a DB
/// test can run it against seeded rows: the template database has already
/// migrated.
pub const BACKFILL_SQL: &str = r#"
WITH created AS (
    INSERT INTO app_environments (app_id, name, kind, build_id)
    SELECT a.id, 'production', 'production', b.id
    FROM apps a
    JOIN app_builds b ON b.id = a.published_build_id
    ON CONFLICT (app_id, name) DO NOTHING
    RETURNING app_id, build_id
), production_events AS (
    INSERT INTO app_environment_events (app_id, environment, build_id, action)
    SELECT app_id, 'production', build_id, 'backfill' FROM created
    RETURNING 1
)
INSERT INTO app_environment_events (app_id, environment, build_id, action)
SELECT c.app_id, 'staging', c.build_id, 'backfill'
FROM created c
WHERE NOT EXISTS (
    SELECT 1 FROM app_environment_events e
    WHERE e.app_id = c.app_id AND e.environment = 'staging' AND e.build_id = c.build_id
);

WITH created AS (
    INSERT INTO app_environments (app_id, name, kind, build_id)
    SELECT a.id, 'staging', 'staging', b.id
    FROM apps a
    JOIN app_builds b ON b.id = a.draft_build_id
    ON CONFLICT (app_id, name) DO NOTHING
    RETURNING app_id, build_id
)
INSERT INTO app_environment_events (app_id, environment, build_id, action)
SELECT c.app_id, 'staging', c.build_id, 'backfill'
FROM created c
WHERE NOT EXISTS (
    SELECT 1 FROM app_environment_events e
    WHERE e.app_id = c.app_id AND e.environment = 'staging' AND e.build_id = c.build_id
);
"#;

/// Puts the pointers back, from the rows.
pub const RESTORE_SQL: &str = r#"
UPDATE apps a SET published_build_id = e.build_id
FROM app_environments e
WHERE e.app_id = a.id AND e.name = 'production';

UPDATE apps a SET draft_build_id = e.build_id
FROM app_environments e
WHERE e.app_id = a.id AND e.name = 'staging';
"#;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        // `BACKFILL_SQL` reads both columns, so run it only while both exist;
        // the single ALTER below drops them together, so a re-run sees both or
        // neither.
        if manager.has_column("apps", "published_build_id").await?
            && manager.has_column("apps", "draft_build_id").await?
        {
            db.execute_unprepared(BACKFILL_SQL).await?;
        }
        db.execute_unprepared(
            "ALTER TABLE apps DROP COLUMN IF EXISTS published_build_id, \
             DROP COLUMN IF EXISTS draft_build_id",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE apps ADD COLUMN IF NOT EXISTS draft_build_id UUID, \
             ADD COLUMN IF NOT EXISTS published_build_id UUID",
        )
        .await?;
        db.execute_unprepared(RESTORE_SQL).await?;
        Ok(())
    }
}
