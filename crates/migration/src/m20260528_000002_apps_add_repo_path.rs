//! Adds `apps.repo_path` to deployments that already have the
//! `apps` table from the historical (pre-squash) migration chain.
//!
//! Fresh deployments get the column from
//! `m20260528_000001_create_customer_apps_schema` directly — the
//! `Table::create().if_not_exists()` call in that migration is a
//! no-op on dev databases that ran the older incremental
//! migrations, but it also means the `repo_path` column is missing
//! on those rows. This migration closes the gap.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Apps {
    Table,
    RepoPath,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.has_column("apps", "repo_path").await? {
            return Ok(());
        }
        manager
            .alter_table(
                Table::alter()
                    .table(Apps::Table)
                    .add_column(ColumnDef::new(Apps::RepoPath).text())
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager.has_column("apps", "repo_path").await? {
            return Ok(());
        }
        manager
            .alter_table(
                Table::alter()
                    .table(Apps::Table)
                    .drop_column(Apps::RepoPath)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
