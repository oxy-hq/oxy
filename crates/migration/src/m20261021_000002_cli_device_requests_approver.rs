use sea_orm_migration::prelude::*;

/// `cli_device_requests.approver_user_id`: the one person who may decide a
/// device-code request, when the CLI named them (`oxyc … --no-browser
/// --approver <email>`).
///
/// The start resolves the email to an active `users` row and stores the id
/// here; the notice `oxy.cli_approval` is sent to that person, and the page's
/// read, approve and deny answer 404 to anyone else. `NULL` is a request that
/// named nobody: decided by whoever types its code, as before. An email that
/// matched no single active user is stored as the nil id, which matches no
/// one, so that request can be decided by nobody and lapses.
///
/// Separate from `m20261021_000001_cli_device_requests`, which may already
/// have run: a shipped migration is never edited.
///
/// **Rollback-safe (expand-only)**: one nullable column, no default, no
/// constraint. The previous deploy never reads it, and a row it writes has it
/// `NULL`, which means "anyone", exactly what that deploy does. A row this
/// deploy bound (a user's id, or nil) is, to the previous deploy, decidable
/// by whoever types the code: no worse than before the column, and the
/// request lapses in ten minutes.
///
/// # Locking
///
/// `ALTER TABLE` takes an ACCESS EXCLUSIVE lock for a catalog-only change on
/// a table that holds only the last hour's requests.
#[derive(DeriveMigrationName)]
pub struct Migration;

const UP_SQL: &str = "ALTER TABLE cli_device_requests \
       ADD COLUMN IF NOT EXISTS approver_user_id UUID";

const DOWN_SQL: &str = "ALTER TABLE cli_device_requests \
       DROP COLUMN IF EXISTS approver_user_id";

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
