use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Projects::Table)
                    .if_not_exists()
                    .col(uuid(Projects::Id).primary_key())
                    .col(string(Projects::Name).not_null())
                    .col(uuid(Projects::OrganizationId).not_null())
                    .col(string(Projects::RepoId))
                    .col(string(Projects::Token))
                    .col(string(Projects::Provider))
                    .col(
                        timestamp_with_time_zone(Projects::CreatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        timestamp_with_time_zone(Projects::UpdatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Projects::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Projects {
    Table,
    Id,
    Name,
    OrganizationId,
    RepoId,
    Token,
    Provider,
    CreatedAt,
    UpdatedAt,
}
