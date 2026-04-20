use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Workspaces {
    Table,
    Status,
    Error,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .add_column(
                        ColumnDef::new(Workspaces::Status)
                            .string_len(20)
                            .not_null()
                            .default("ready"),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .add_column(ColumnDef::new(Workspaces::Error).text().null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .drop_column(Workspaces::Error)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .drop_column(Workspaces::Status)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
