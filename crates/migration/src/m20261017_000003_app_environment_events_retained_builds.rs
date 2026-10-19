use sea_orm_migration::prelude::*;

/// A `staging / backfill` event for every retained build that predates the
/// environment events — so each still counts as a build staging served.
///
/// Every move of production's pointer now asks invariant 1 (production only
/// ever serves a build staging has served), and its evidence is a `staging`
/// row in `app_environment_events` (environments design §5.5a, ruling 3). The
/// events began with `m20260922_000001_app_environments`, whose backfill wrote
/// one for the two builds each app's pointers named that day — and none for
/// the up to ten older builds retention keeps, which the console's *Make Live*
/// of an older build (its rollback) could make live until then. Every one of
/// them was staging's: before the events, every publish moved the draft
/// pointer. Without a row, the emergency rollback to the build before the last
/// release would answer `409 staging_never_served`.
///
/// Which builds: those with **no event at all** and **no `published_to`**. A
/// build published since the events has its own `publish` event; one published
/// since `m20261015_000001_app_builds_published_to` has `published_to` too; a
/// sandbox's build always has its sandbox's event (sandboxes postdate the
/// events). A build a sandbox agent token published is skipped by name
/// (`published_token_id`): it never stands in for staging. `at` is the build's
/// own creation, when the draft pointer moved to it.
///
/// **Rollback-safe**: rows in a table both deploys write, no schema change.
/// Idempotent: a build that has any event is never chosen again. `down` keeps
/// the rows — they are true history, and the previous deploy never reads them
/// for anything but a rollback target it already offered.
#[derive(DeriveMigrationName)]
pub struct Migration;

pub const BACKFILL_SQL: &str = r#"
INSERT INTO app_environment_events (app_id, environment, build_id, action, at)
SELECT b.app_id, 'staging', b.id, 'backfill', b.created_at
FROM app_builds b
WHERE b.published_to IS NULL
  AND b.published_token_id IS NULL
  AND NOT EXISTS (
      SELECT 1 FROM app_environment_events e WHERE e.build_id = b.id
  );
"#;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(BACKFILL_SQL)
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
