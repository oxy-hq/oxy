use sea_orm_migration::prelude::*;

/// A person can mute a platform notice (an `oxy.*` kind) per channel.
///
/// # Why a new table
///
/// `notification_mutes` cannot hold these. Its `app_id` is `NOT NULL` with a
/// foreign key to `apps`, and a platform notice has no app. Relaxing the column
/// would also make `(user, NULL, kind, channel)` a primary key Postgres cannot
/// enforce. So platform mutes are keyed by (user, kind, channel) alone.
///
/// A platform notice stores no content of its own (it is rendered from Oxy's
/// records when it is read or delivered), so this table, like the rest of the
/// signal tables, holds ids, kinds and a channel name.
///
/// # Rollback
///
/// Additive: a new table the previous deploy never reads. Its mutes simply do
/// not apply on a re-pinned binary, which also sends no platform notices.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE IF NOT EXISTS notification_platform_mutes (\
                   user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE, \
                   kind TEXT NOT NULL, \
                   channel TEXT NOT NULL CHECK (channel IN ('push','email','slack')), \
                   created_at TIMESTAMPTZ NOT NULL DEFAULT now(), \
                   PRIMARY KEY (user_id, kind, channel)\
                 )",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS notification_platform_mutes")
            .await?;
        Ok(())
    }
}
