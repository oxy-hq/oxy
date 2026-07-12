use sea_orm_migration::prelude::*;

/// Adds composite indexes that back the hot thread-open query path.
///
/// `messages(thread_id)` had a foreign key but no index, so opening a thread
/// (`WHERE thread_id = ? ORDER BY created_at`) was a full table scan. `threads`
/// is filtered by `user_id` and ordered by `created_at`, but only `user_id` was
/// indexed. Both queries now hit a covering composite index.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Messages {
    Table,
    ThreadId,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Threads {
    Table,
    UserId,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Composite index for `WHERE thread_id = ? ORDER BY created_at`.
        manager
            .create_index(
                Index::create()
                    .table(Messages::Table)
                    .name("idx_messages_thread_id_created_at")
                    .col(Messages::ThreadId)
                    .col(Messages::CreatedAt)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // Composite index for `WHERE user_id = ? ORDER BY created_at`.
        manager
            .create_index(
                Index::create()
                    .table(Threads::Table)
                    .name("idx_threads_user_id_created_at")
                    .col(Threads::UserId)
                    .col(Threads::CreatedAt)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .table(Threads::Table)
                    .name("idx_threads_user_id_created_at")
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .table(Messages::Table)
                    .name("idx_messages_thread_id_created_at")
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
