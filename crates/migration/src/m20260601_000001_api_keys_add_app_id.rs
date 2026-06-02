//! Add a nullable `app_id` FK on `api_keys` so a key can be scoped to a
//! specific customer-app. When the app row is deleted, its bound keys go
//! with it (ON DELETE CASCADE). Existing keys (CLI-style, no app binding)
//! stay null.

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ApiKeys::Table)
                    .add_column(uuid_null(ApiKeys::AppId))
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk_api_keys_app_id")
                    .from(ApiKeys::Table, ApiKeys::AppId)
                    .to(Apps::Table, Apps::Id)
                    .on_delete(ForeignKeyAction::Cascade)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(ApiKeys::Table)
                    .name("idx_api_keys_app_id")
                    .col(ApiKeys::AppId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .table(ApiKeys::Table)
                    .name("idx_api_keys_app_id")
                    .to_owned(),
            )
            .await?;
        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .table(ApiKeys::Table)
                    .name("fk_api_keys_app_id")
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(ApiKeys::Table)
                    .drop_column(ApiKeys::AppId)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum ApiKeys {
    Table,
    AppId,
}

#[derive(DeriveIden)]
enum Apps {
    Table,
    Id,
}
