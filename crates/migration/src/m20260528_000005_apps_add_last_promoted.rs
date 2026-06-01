//! Adds `apps.last_promoted_by` + `apps.last_promoted_at` — who last made a
//! build live (promote draft or Make Live/rollback) and when. Closes the
//! audit gap where `app_builds.published_by` only captured the *original*
//! `oxy publish`, so a build promoted by a different admin still showed its
//! original publisher.
//!
//! `last_promoted_by` is a FK → `users` `ON DELETE SET NULL` so a deleted
//! user leaves a clean NULL. Both nullable; guarded with `has_column`.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Apps {
    Table,
    LastPromotedBy,
    LastPromotedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

const FK_NAME: &str = "fk_apps_last_promoted_by";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager.has_column("apps", "last_promoted_by").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(Apps::Table)
                        .add_column(ColumnDef::new(Apps::LastPromotedBy).uuid())
                        .to_owned(),
                )
                .await?;
            manager
                .create_foreign_key(
                    ForeignKey::create()
                        .name(FK_NAME)
                        .from(Apps::Table, Apps::LastPromotedBy)
                        .to(Users::Table, Users::Id)
                        .on_delete(ForeignKeyAction::SetNull)
                        .on_update(ForeignKeyAction::NoAction)
                        .to_owned(),
                )
                .await?;
        }
        if !manager.has_column("apps", "last_promoted_at").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(Apps::Table)
                        .add_column(ColumnDef::new(Apps::LastPromotedAt).timestamp_with_time_zone())
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.has_column("apps", "last_promoted_at").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(Apps::Table)
                        .drop_column(Apps::LastPromotedAt)
                        .to_owned(),
                )
                .await?;
        }
        if manager.has_column("apps", "last_promoted_by").await? {
            manager
                .drop_foreign_key(
                    ForeignKey::drop()
                        .table(Apps::Table)
                        .name(FK_NAME)
                        .to_owned(),
                )
                .await?;
            manager
                .alter_table(
                    Table::alter()
                        .table(Apps::Table)
                        .drop_column(Apps::LastPromotedBy)
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
