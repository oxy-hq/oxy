use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Branches {
    Table,
    Id,
    ProjectId,
    Name,
    Revision,
    SyncStatus,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum WorkspaceRepos {
    Table,
    Id,
    RepoId,
    GitNamespaceId,
    CreatedAt,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(WorkspaceRepos::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Branches::Table).to_owned())
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Branches::Table)
                    .if_not_exists()
                    .col(uuid(Branches::Id).primary_key())
                    .col(uuid(Branches::ProjectId).not_null())
                    .col(string_len(Branches::Name, 255).not_null())
                    .col(char_len(Branches::Revision, 40).not_null())
                    .col(string(Branches::SyncStatus).not_null())
                    .col(timestamp_with_time_zone(Branches::CreatedAt).not_null())
                    .col(timestamp_with_time_zone(Branches::UpdatedAt).not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(WorkspaceRepos::Table)
                    .if_not_exists()
                    .col(uuid(WorkspaceRepos::Id).primary_key())
                    .col(string(WorkspaceRepos::RepoId).not_null())
                    .col(uuid(WorkspaceRepos::GitNamespaceId).not_null())
                    .col(
                        timestamp_with_time_zone(WorkspaceRepos::CreatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        timestamp_with_time_zone(WorkspaceRepos::UpdatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
