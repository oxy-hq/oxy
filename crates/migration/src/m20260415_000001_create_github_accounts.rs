use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum GithubAccounts {
    Table,
    Id,
    UserId,
    GithubLogin,
    OauthToken,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(GithubAccounts::Table)
                    .if_not_exists()
                    .col(uuid(GithubAccounts::Id).primary_key())
                    .col(uuid(GithubAccounts::UserId).not_null().unique_key())
                    .col(string(GithubAccounts::GithubLogin).not_null())
                    .col(string(GithubAccounts::OauthToken).not_null())
                    .col(
                        timestamp_with_time_zone(GithubAccounts::CreatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        timestamp_with_time_zone(GithubAccounts::UpdatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_github_accounts_user_id")
                            .from(GithubAccounts::Table, GithubAccounts::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(GithubAccounts::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
