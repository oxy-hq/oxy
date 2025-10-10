use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ProjectRepos::Table)
                    .if_not_exists()
                    .col(uuid(ProjectRepos::Id).primary_key())
                    .col(string(ProjectRepos::RepoId).not_null())
                    .col(uuid(ProjectRepos::GitNamespaceId).not_null())
                    .col(
                        timestamp_with_time_zone(ProjectRepos::CreatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        timestamp_with_time_zone(ProjectRepos::UpdatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Projects::Table)
                    .drop_column(Alias::new("repo_id"))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Projects::Table)
                    .drop_column(Alias::new("token"))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Projects::Table)
                    .drop_column(Alias::new("provider"))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Projects::Table)
                    .add_column(uuid_null(Projects::ProjectRepoId).null())
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Projects::Table)
                    .drop_column(Projects::ProjectRepoId)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Projects::Table)
                    .add_column(string(Alias::new("repo_id")).default(""))
                    .add_column(string(Alias::new("token")).default(""))
                    .add_column(string(Alias::new("provider")).default(""))
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table(ProjectRepos::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum ProjectRepos {
    Table,
    Id,
    RepoId,
    Token,
    Provider,
    CreatedAt,
    UpdatedAt,
    GitNamespaceId,
}

#[derive(DeriveIden)]
enum Projects {
    Table,
    ProjectRepoId,
}
