use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create settings table for GitHub integration
        // This table should have 0 or 1 row - only one GitHub repository is supported for the whole app
        manager
            .create_table(
                Table::create()
                    .table(Settings::Table)
                    .if_not_exists()
                    .col(pk_auto(Settings::Id))
                    .col(text(Settings::GithubToken)) // Encrypted GitHub token
                    .col(big_integer_null(Settings::SelectedRepoId)) // GitHub repository ID
                    .col(boolean(Settings::Onboarded).default(false))
                    .col(text_null(Settings::Revision)) // Current revision/commit hash of the synced repo
                    .col(string_len(Settings::SyncStatus, 20)) // Sync status enum: idle, syncing, synced, error
                    .col(
                        timestamp_with_time_zone(Settings::CreatedAt)
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .col(
                        timestamp_with_time_zone(Settings::UpdatedAt)
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Settings::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Settings {
    Table,
    Id,
    GithubToken,
    SelectedRepoId,
    Revision,
    SyncStatus,
    CreatedAt,
    UpdatedAt,
    Onboarded,
}
