use sea_orm_migration::prelude::*;

/// `app_environments.schedules_enabled`: whether a non-production environment
/// of a custom app fires its functions' `schedule`s
/// (`internal-docs/2026-09-10-custom-app-environments-design.md` §4.5,
/// ruling 1).
///
/// Off by default, for staging and sandboxes alike: a schedule outside
/// production reads production data on a timer and calls third parties with
/// staging's credentials, so an environment fires one only once staff turned
/// it on (`PUT /api/customer-apps/{id}/environments/{name}/schedules`). With
/// the switch off the environment has no `agentic_schedules` rows at all.
/// Production has no switch and never reads the column: its schedules always
/// run, as before.
///
/// Control-plane state about an environment — no row of an org's data.
///
/// **Rollback-safe**: one column with a constant default, no constraint on
/// the existing table. The previous deploy never reads or writes it, and a
/// row it inserts (a sandbox, a fixed environment's first pointer move) takes
/// the default — off.
#[derive(DeriveMigrationName)]
pub struct Migration;

const UP_SQL: &str = "ALTER TABLE app_environments \
     ADD COLUMN IF NOT EXISTS schedules_enabled BOOLEAN NOT NULL DEFAULT false";

const DOWN_SQL: &str = "ALTER TABLE app_environments DROP COLUMN IF EXISTS schedules_enabled";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(UP_SQL).await?;
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
