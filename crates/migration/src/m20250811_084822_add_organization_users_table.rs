use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(OrganizationUsers::Table)
                    .if_not_exists()
                    .col(uuid(OrganizationUsers::Id).primary_key())
                    .col(uuid(OrganizationUsers::OrganizationId))
                    .col(uuid(OrganizationUsers::UserId))
                    .col(string(OrganizationUsers::Role))
                    .col(
                        timestamp_with_time_zone(OrganizationUsers::CreatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        timestamp_with_time_zone(OrganizationUsers::UpdatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(OrganizationUsers::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum OrganizationUsers {
    Table,
    Id,
    OrganizationId,
    UserId,
    Role,
    CreatedAt,
    UpdatedAt,
}
