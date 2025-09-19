use sea_orm_migration::{prelude::*, schema::*, sea_orm::DbBackend};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();

        let default_expr = match backend {
            DbBackend::Postgres => Expr::cust("'00000000-0000-0000-0000-000000000000'::uuid"),
            DbBackend::Sqlite => Expr::cust("x'00000000000000000000000000000000'"),
            DbBackend::MySql => Expr::cust("'00000000-0000-0000-0000-000000000000'::uuid"),
        };

        manager
            .alter_table(
                Table::alter()
                    .table(Projects::Table)
                    .add_column(
                        uuid(Projects::ActiveBranchId)
                            .not_null()
                            .default(default_expr),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Projects::Table)
                    .drop_column(Projects::ActiveBranchId)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Projects {
    Table,
    ActiveBranchId,
}
