use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(Index::drop().name("idx_unique_source_runindex").to_owned())
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_unique_source_runindex")
                    .table(Runs::Table)
                    .col(Runs::ProjectId)
                    .col(Runs::BranchId)
                    .col(Runs::SourceId)
                    .col(Runs::RunIndex)
                    .cond_where(Expr::col(Runs::RunIndex).is_not_null())
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(Index::drop().name("idx_unique_source_runindex").to_owned())
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_unique_source_runindex")
                    .table(Runs::Table)
                    .col(Runs::SourceId)
                    .col(Runs::RunIndex)
                    .cond_where(Expr::col(Runs::RunIndex).is_not_null())
                    .unique()
                    .to_owned(),
            )
            .await
    }
}

#[derive(Iden)]
enum Runs {
    Table,
    ProjectId,
    BranchId,
    SourceId,
    RunIndex,
}
