use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Logs::Table)
                    .if_not_exists()
                    .col(uuid(Logs::Id).primary_key())
                    .col(uuid(Logs::UserId))
                    .col(text(Logs::Prompts))
                    .col(uuid(Logs::ThreadId))
                    .col(json(Logs::Log))
                    .col(timestamp_with_time_zone(Logs::CreatedAt))
                    .col(timestamp_with_time_zone(Logs::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_logs_user_id")
                            .from(Logs::Table, Logs::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::NoAction),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_logs_thread_id")
                            .from(Logs::Table, Logs::ThreadId)
                            .to(Threads::Table, Threads::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::NoAction),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Logs::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Logs {
    Table,
    Id,
    UserId,
    Prompts,
    ThreadId,
    Log,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Threads {
    Table,
    Id,
}
