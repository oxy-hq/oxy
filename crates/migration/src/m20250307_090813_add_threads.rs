use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Threads::Table)
                    .if_not_exists()
                    .col(uuid(Threads::Id).primary_key())
                    .col(string(Threads::Title))
                    .col(string(Threads::Question))
                    .col(string(Threads::Answer))
                    .col(string(Threads::Agent))
                    .col(
                        timestamp_with_time_zone(Threads::CreatedAt)
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Threads::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Threads {
    Table,
    Id,
    Title,
    CreatedAt,
    Question,
    Answer,
    Agent,
}
