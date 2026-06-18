//! Org-level uploadable logo (white-labels the workspace HQ chrome — the
//! rail tile + HQ heading). Stored inline on the organization row: the raw
//! image `logo` (bytea) plus its `logo_content_type` (e.g. `image/png`).
//! Both nullable — an org with neither falls back to the code-first
//! `logo.*` file at the workspace root, then to the name initial.
//!
//! Inline bytes (not S3/disk) keep this working identically in local and
//! cloud and avoid new infra; logos are small and capped by the upload
//! handler.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Logo,
    LogoContentType,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Organizations::Table)
                    .add_column(ColumnDef::new(Organizations::Logo).binary().null())
                    .add_column(ColumnDef::new(Organizations::LogoContentType).text().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Organizations::Table)
                    .drop_column(Organizations::Logo)
                    .drop_column(Organizations::LogoContentType)
                    .to_owned(),
            )
            .await
    }
}
