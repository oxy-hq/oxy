use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // SQLite doesn't support adding foreign keys to existing tables
        // We need to recreate the table with the foreign key constraint

        // Create a temporary table with the new structure including foreign key
        manager
            .create_table(
                Table::create()
                    .table(ThreadsNew::Table)
                    .if_not_exists()
                    .col(uuid(ThreadsNew::Id).primary_key())
                    .col(string(ThreadsNew::Title).not_null())
                    .col(string(ThreadsNew::Output).not_null())
                    .col(string(ThreadsNew::Input).not_null())
                    .col(string(ThreadsNew::SourceType).not_null())
                    .col(string(ThreadsNew::Source).not_null())
                    .col(
                        timestamp_with_time_zone(ThreadsNew::CreatedAt)
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .col(text(ThreadsNew::References).not_null())
                    .col(uuid_null(ThreadsNew::UserId))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_threads_user_id")
                            .from(ThreadsNew::Table, ThreadsNew::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::NoAction),
                    )
                    .to_owned(),
            )
            .await?;

        // Copy data from old table to new table
        manager
            .get_connection()
            .execute_unprepared(
                "INSERT INTO threads_new (id, title, output, input, source_type, source, created_at, references, user_id) 
                 SELECT id, title, output, input, source_type, source, created_at, references, NULL FROM threads"
            )
            .await?;

        // Drop the old table
        manager
            .drop_table(Table::drop().table(Threads::Table).to_owned())
            .await?;

        // Rename the new table to the original name
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE threads_new RENAME TO threads")
            .await?;

        // Create index on user_id for faster lookups
        manager
            .create_index(
                Index::create()
                    .table(Threads::Table)
                    .name("idx_threads_user_id")
                    .col(Threads::UserId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop the index
        manager
            .drop_index(
                Index::drop()
                    .table(Threads::Table)
                    .name("idx_threads_user_id")
                    .to_owned(),
            )
            .await?;

        // Recreate the original threads table without user_id and foreign key
        manager
            .create_table(
                Table::create()
                    .table(ThreadsOld::Table)
                    .if_not_exists()
                    .col(uuid(ThreadsOld::Id).primary_key())
                    .col(string(ThreadsOld::Title).not_null())
                    .col(string(ThreadsOld::Output).not_null())
                    .col(string(ThreadsOld::Input).not_null())
                    .col(string(ThreadsOld::SourceType).not_null())
                    .col(string(ThreadsOld::Source).not_null())
                    .col(
                        timestamp_with_time_zone(ThreadsOld::CreatedAt)
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .col(text(ThreadsOld::References).not_null())
                    .to_owned(),
            )
            .await?;

        // Copy data back (excluding user_id)
        manager
            .get_connection()
            .execute_unprepared(
                "INSERT INTO threads_old (id, title, output, input, source_type, source, created_at, references) 
                 SELECT id, title, output, input, source_type, source, created_at, references FROM threads"
            )
            .await?;

        // Drop the current threads table
        manager
            .drop_table(Table::drop().table(Threads::Table).to_owned())
            .await?;

        // Rename back
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE threads_old RENAME TO threads")
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Threads {
    Table,
    Id,
    Title,
    Output,
    Input,
    SourceType,
    Source,
    CreatedAt,
    References,
    UserId,
}

#[derive(DeriveIden)]
enum ThreadsNew {
    Table,
    Id,
    Title,
    Output,
    Input,
    SourceType,
    Source,
    CreatedAt,
    References,
    UserId,
}

#[derive(DeriveIden)]
enum ThreadsOld {
    Table,
    Id,
    Title,
    Output,
    Input,
    SourceType,
    Source,
    CreatedAt,
    References,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
