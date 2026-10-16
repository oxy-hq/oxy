use sea_orm_migration::prelude::*;

/// Notification categories: a thread per notification, and mutes above a kind.
///
/// # Why
///
/// Every notification sits in a tree — source (Oxygen or one custom app) →
/// category path → kind — the shape iOS categories and thread ids and Android
/// channel groups and channels need (`internal-docs/custom-app-notifications.md`).
/// The tree itself is config: an app declares it in its build's `oxy-app.json`
/// and Oxygen's is in code, so neither needs a table. Two facts do:
///
/// - `notifications.thread_key` — the thread an app put a notification in
///   (`ctx.notify({ thread })`), so a client can collapse it and a phone can
///   group it. Only a hash of the app's text is stored: SHA-256, hex, first 32
///   characters. The text is the app's and may name a customer.
/// - `notification_scope_mutes` — a channel a person turned off for a whole
///   source (`scope = ''`) or a category subtree (`scope = 'a/b'`). Kind-level
///   mutes keep their tables (`notification_mutes`,
///   `notification_platform_mutes`). `app_id` is `NULL` for Oxygen, so the two
///   uniqueness rules are partial indexes: a primary key cannot hold a `NULL`.
///
/// Ids, a path, a channel name and a hash: control-plane bookkeeping, no
/// content (`data_placement.rs`).
///
/// # Rollback
///
/// Additive. A nullable column the previous deploy never reads (its entity
/// does not name it, and it writes rows without it, which `NULL` accepts) and
/// a new table it never reads, so a re-pinned binary ignores both: its
/// deliveries simply do not honour a category mute, and nothing is lost.
///
/// # Locking
///
/// `ALTER TABLE notifications` takes an ACCESS EXCLUSIVE lock for a
/// catalog-only change (a nullable column with no default).
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("ALTER TABLE notifications ADD COLUMN IF NOT EXISTS thread_key TEXT")
            .await?;
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS notification_scope_mutes (\
               id UUID PRIMARY KEY, \
               user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE, \
               app_id UUID REFERENCES apps(id) ON DELETE CASCADE, \
               scope TEXT NOT NULL, \
               channel TEXT NOT NULL CHECK (channel IN ('push','email','slack')), \
               created_at TIMESTAMPTZ NOT NULL DEFAULT now()\
             )",
        )
        .await?;
        db.execute_unprepared(
            "CREATE UNIQUE INDEX IF NOT EXISTS notification_scope_mutes_app_uidx \
               ON notification_scope_mutes (user_id, app_id, scope, channel) \
               WHERE app_id IS NOT NULL",
        )
        .await?;
        db.execute_unprepared(
            "CREATE UNIQUE INDEX IF NOT EXISTS notification_scope_mutes_oxygen_uidx \
               ON notification_scope_mutes (user_id, scope, channel) \
               WHERE app_id IS NULL",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE IF EXISTS notification_scope_mutes")
            .await?;
        db.execute_unprepared("ALTER TABLE notifications DROP COLUMN IF EXISTS thread_key")
            .await?;
        Ok(())
    }
}
