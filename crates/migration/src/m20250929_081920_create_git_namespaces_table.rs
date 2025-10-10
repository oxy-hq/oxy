use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(GitNamespaces::Table)
                    .if_not_exists()
                    .col(uuid(GitNamespaces::Id).primary_key())
                    .col(big_integer(GitNamespaces::InstallationId))
                    .col(string(GitNamespaces::Name))
                    .col(string(GitNamespaces::OwnerType))
                    .col(string(GitNamespaces::Provider))
                    .col(string(GitNamespaces::Slug))
                    .col(uuid(GitNamespaces::UserId))
                    .col(string(GitNamespaces::OauthToken))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(GitNamespaces::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum GitNamespaces {
    Table,
    Id,
    InstallationId,
    Name,
    OwnerType,
    Provider,
    Slug,
    UserId,
    OauthToken,
}
