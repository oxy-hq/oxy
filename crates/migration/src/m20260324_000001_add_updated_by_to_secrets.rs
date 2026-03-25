use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Secrets::Table)
                    .add_column(ColumnDef::new(Secrets::UpdatedBy).uuid().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Secrets::Table)
                    .drop_column(Secrets::UpdatedBy)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Secrets {
    Table,
    UpdatedBy,
}
