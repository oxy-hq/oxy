use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Conversations::Table)
                    .if_not_exists()
                    .col(uuid(Conversations::Id).primary_key())
                    .col(string(Conversations::Title))
                    .col(
                        timestamp_with_time_zone(Conversations::CreatedAt)
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .col(
                        timestamp_with_time_zone(Conversations::UpdatedAt)
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .col(timestamp_with_time_zone_null(Conversations::DeletedAt).null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Conversations::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Conversations {
    Table,
    Id,
    Title,
    CreatedAt,
    UpdatedAt,
    DeletedAt,
}
