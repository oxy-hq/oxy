use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Messages::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Conversations::Table).to_owned())
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
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

#[derive(DeriveIden)]
enum Conversations {
    Table,
    Id,
    Title,
    CreatedAt,
    UpdatedAt,
    DeletedAt,
    Agent,
}
