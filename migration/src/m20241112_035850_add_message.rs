use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Messages::Table)
                    .if_not_exists()
                    .col(uuid(Messages::Id).primary_key())
                    .col(uuid(Messages::ConversationId))
                    .foreign_key(
                        ForeignKeyCreateStatement::new()
                            .from_col(Messages::ConversationId)
                            .to(Conversations::Table, Conversations::Id),
                    )
                    .col(string(Messages::Content))
                    .col(boolean(Messages::IsHuman))
                    .col(
                        timestamp_with_time_zone(Messages::CreatedAt)
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Messages::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Conversations {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Messages {
    Table,
    Id,
    ConversationId,
    Content,
    IsHuman,
    CreatedAt,
}
