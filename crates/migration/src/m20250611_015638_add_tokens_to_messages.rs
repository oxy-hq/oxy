use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let _ = manager
            .alter_table(
                Table::alter()
                    .table(Messages::Table)
                    .add_column(unsigned(Messages::InputTokens).default(0))
                    .to_owned(),
            )
            .await;
        let _ = manager
            .alter_table(
                Table::alter()
                    .table(Messages::Table)
                    .add_column(unsigned(Messages::OutputTokens).default(0))
                    .to_owned(),
            )
            .await;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let _ = manager
            .alter_table(
                Table::alter()
                    .table(Messages::Table)
                    .drop_column(Messages::InputTokens)
                    .to_owned(),
            )
            .await;
        let _ = manager
            .alter_table(
                Table::alter()
                    .table(Messages::Table)
                    .drop_column(Messages::OutputTokens)
                    .to_owned(),
            )
            .await;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Messages {
    Table,
    InputTokens,
    OutputTokens,
}
