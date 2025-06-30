use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add status column to users table for soft delete functionality
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(string(Users::Status).default("active").not_null())
                    .to_owned(),
            )
            .await?;

        // Create index on status for faster filtering
        manager
            .create_index(
                Index::create()
                    .table(Users::Table)
                    .name("idx_users_status")
                    .col(Users::Status)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop the index first
        manager
            .drop_index(
                Index::drop()
                    .table(Users::Table)
                    .name("idx_users_status")
                    .to_owned(),
            )
            .await?;

        // Drop the status column
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .drop_column(Users::Status)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Status,
}
