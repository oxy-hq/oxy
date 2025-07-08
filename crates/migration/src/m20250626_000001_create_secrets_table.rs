use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create secrets table
        manager
            .create_table(
                Table::create()
                    .table(Secrets::Table)
                    .if_not_exists()
                    .col(uuid(Secrets::Id).primary_key())
                    .col(string(Secrets::Name))
                    .col(text(Secrets::EncryptedValue))
                    .col(text_null(Secrets::Description))
                    .col(
                        timestamp_with_time_zone(Secrets::CreatedAt)
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .col(
                        timestamp_with_time_zone(Secrets::UpdatedAt)
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .col(uuid(Secrets::CreatedBy))
                    .col(boolean(Secrets::IsActive).default(true))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_secrets_created_by")
                            .from(Secrets::Table, Secrets::CreatedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create index on created_by for user-specific queries
        manager
            .create_index(
                Index::create()
                    .table(Secrets::Table)
                    .name("idx_secrets_created_by")
                    .col(Secrets::CreatedBy)
                    .to_owned(),
            )
            .await?;

        // Create index on is_active for soft deletion queries
        manager
            .create_index(
                Index::create()
                    .table(Secrets::Table)
                    .name("idx_secrets_is_active")
                    .col(Secrets::IsActive)
                    .to_owned(),
            )
            .await?;

        // Create composite index for efficient active secrets lookup
        manager
            .create_index(
                Index::create()
                    .table(Secrets::Table)
                    .name("idx_secrets_name_active")
                    .col(Secrets::Name)
                    .col(Secrets::IsActive)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Secrets::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Secrets {
    Table,
    Id,
    Name,
    EncryptedValue,
    Description,
    CreatedAt,
    UpdatedAt,
    CreatedBy,
    IsActive,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
