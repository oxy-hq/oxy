use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create api_keys table
        manager
            .create_table(
                Table::create()
                    .table(ApiKeys::Table)
                    .if_not_exists()
                    .col(uuid(ApiKeys::Id).primary_key())
                    .col(uuid(ApiKeys::UserId))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_api_keys_user_id")
                            .from(ApiKeys::Table, ApiKeys::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(string(ApiKeys::KeyHash))
                    .col(string(ApiKeys::Name))
                    .col(timestamp_with_time_zone_null(ApiKeys::ExpiresAt))
                    .col(timestamp_with_time_zone_null(ApiKeys::LastUsedAt))
                    .col(
                        timestamp_with_time_zone(ApiKeys::CreatedAt)
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .col(
                        timestamp_with_time_zone(ApiKeys::UpdatedAt)
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .col(boolean(ApiKeys::IsActive).default(true))
                    .to_owned(),
            )
            .await?;

        // Create index on user_id for faster lookups
        manager
            .create_index(
                Index::create()
                    .table(ApiKeys::Table)
                    .name("idx_api_keys_user_id")
                    .col(ApiKeys::UserId)
                    .to_owned(),
            )
            .await?;

        // Create index on key_hash for authentication
        manager
            .create_index(
                Index::create()
                    .table(ApiKeys::Table)
                    .name("idx_api_keys_key_hash")
                    .col(ApiKeys::KeyHash)
                    .to_owned(),
            )
            .await?;

        // Create index on is_active for filtering active keys
        manager
            .create_index(
                Index::create()
                    .table(ApiKeys::Table)
                    .name("idx_api_keys_is_active")
                    .col(ApiKeys::IsActive)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ApiKeys::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum ApiKeys {
    Table,
    Id,
    UserId,
    KeyHash,
    Name,
    ExpiresAt,
    LastUsedAt,
    CreatedAt,
    UpdatedAt,
    IsActive,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
