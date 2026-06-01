//! Adds the `app_builds` table (versioned customer-app bundles) and the
//! `apps.draft_build_id` / `apps.published_build_id` channel pointers.
//!
//! Additive on top of `m20260528_000001_create_customer_apps_schema`
//! (and `_000002_apps_add_repo_path`): the new publish pipeline stores
//! each build under `customer-apps/<app_id>/builds/<build_id>/` in S3 and
//! points each channel at a build row. Legacy `s3` rows keep both
//! pointers NULL and continue to serve from the state dir until migrated.
//! Column adds are guarded with `has_column` so dev databases that
//! already ran the squashed schema pick them up cleanly.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum AppBuilds {
    Table,
    Id,
    AppId,
    BuildId,
    S3Prefix,
    ManifestJson,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Apps {
    Table,
    Id,
    DraftBuildId,
    PublishedBuildId,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AppBuilds::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AppBuilds::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AppBuilds::AppId).uuid().not_null())
                    .col(ColumnDef::new(AppBuilds::BuildId).text().not_null())
                    .col(ColumnDef::new(AppBuilds::S3Prefix).text().not_null())
                    .col(ColumnDef::new(AppBuilds::ManifestJson).json_binary())
                    .col(
                        ColumnDef::new(AppBuilds::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_app_builds_app_id")
                            .from(AppBuilds::Table, AppBuilds::AppId)
                            .to(Apps::Table, Apps::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_app_builds_app_id")
                    .table(AppBuilds::Table)
                    .col(AppBuilds::AppId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("uq_app_builds_app_build")
                    .table(AppBuilds::Table)
                    .col(AppBuilds::AppId)
                    .col(AppBuilds::BuildId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        if !manager.has_column("apps", "draft_build_id").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(Apps::Table)
                        .add_column(ColumnDef::new(Apps::DraftBuildId).uuid())
                        .to_owned(),
                )
                .await?;
        }
        if !manager.has_column("apps", "published_build_id").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(Apps::Table)
                        .add_column(ColumnDef::new(Apps::PublishedBuildId).uuid())
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.has_column("apps", "draft_build_id").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(Apps::Table)
                        .drop_column(Apps::DraftBuildId)
                        .to_owned(),
                )
                .await?;
        }
        if manager.has_column("apps", "published_build_id").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(Apps::Table)
                        .drop_column(Apps::PublishedBuildId)
                        .to_owned(),
                )
                .await?;
        }
        manager
            .drop_table(Table::drop().table(AppBuilds::Table).if_exists().to_owned())
            .await?;
        Ok(())
    }
}
