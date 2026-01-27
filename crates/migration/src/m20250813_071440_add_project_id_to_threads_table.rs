use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let default_expr = Expr::cust("'00000000-0000-0000-0000-000000000000'::uuid");

        manager
            .alter_table(
                Table::alter()
                    .table(Threads::Table)
                    .add_column(uuid(Threads::ProjectId).not_null().default(default_expr))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Threads::Table)
                    .drop_column(Threads::ProjectId)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Threads {
    Table,
    ProjectId,
}
