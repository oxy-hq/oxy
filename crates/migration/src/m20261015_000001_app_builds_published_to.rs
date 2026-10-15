use sea_orm_migration::prelude::*;

/// `app_builds.published_to`: the environment a publish moved first —
/// `staging` for a publish to the app's channels (with or without promote,
/// and an agent's draft), `dev-<handle>` for a publish to a sandbox
/// (`internal-docs/custom-app-sandboxes.md` → §3 Lifecycle, and
/// `internal-docs/2026-09-10-custom-app-environments-design.md` §5).
///
/// Written with the build row, before any pointer moves, so two questions
/// stop depending on whether a pointer event has been recorded yet:
///
/// - **Retention.** A build only a sandbox ever served is pruned in a window
///   of its own. Decided by events alone, a sandbox build whose pointer move
///   had not committed yet counted as a channel build, and a concurrent
///   publish's pruning could drop the channel's tenth-newest build for it.
/// - **Read-back.** A build that nothing serves any more — a deleted
///   sandbox's, or one staging moved off — was not known to be
///   non-production, so naming it answered a caller without reach an empty
///   list instead of the refusal a served build gets.
///
/// Backfilled from each build's first `publish` event, which names the same
/// environment (a channel publish records `staging` first). `NULL` for a
/// build with no publish event: one published before events existed, which
/// both readers keep treating as they did.
///
/// **Rollback-safe**: one nullable column, no constraint — a `CHECK` added to
/// a table the previous deploy writes is a contraction the rollback-safety
/// guard refuses (`internal-docs/revert-safe-migrations.md`). That deploy
/// never reads the column, and a build it publishes is `NULL`.
#[derive(DeriveMigrationName)]
pub struct Migration;

const UP_SQL: &str = "ALTER TABLE app_builds ADD COLUMN IF NOT EXISTS published_to TEXT";

const BACKFILL_SQL: &str = "UPDATE app_builds b SET published_to = first.environment \
       FROM (SELECT DISTINCT ON (build_id) build_id, environment \
               FROM app_environment_events \
              WHERE action = 'publish' AND build_id IS NOT NULL \
                AND (environment = 'staging' OR environment LIKE 'dev-%') \
              ORDER BY build_id, at) first \
      WHERE b.id = first.build_id AND b.published_to IS NULL";

const DOWN_SQL: &str = "ALTER TABLE app_builds DROP COLUMN IF EXISTS published_to";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(UP_SQL).await?;
        db.execute_unprepared(BACKFILL_SQL).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(DOWN_SQL)
            .await?;
        Ok(())
    }
}
