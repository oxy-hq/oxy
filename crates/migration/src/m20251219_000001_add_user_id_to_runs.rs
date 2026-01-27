use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add user_id column to runs table (nullable for existing records)
        manager
            .alter_table(
                Table::alter()
                    .table(Runs::Table)
                    .add_column(uuid_null(Runs::UserId))
                    .to_owned(),
            )
            .await?;

        // Add foreign key constraint
        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk_runs_user_id")
                    .from(Runs::Table, Runs::UserId)
                    .to(Users::Table, Users::Id)
                    .on_delete(ForeignKeyAction::Cascade)
                    .on_update(ForeignKeyAction::NoAction)
                    .to_owned(),
            )
            .await?;

        // Create index on user_id for faster lookups
        manager
            .create_index(
                Index::create()
                    .table(Runs::Table)
                    .name("idx_runs_user_id")
                    .col(Runs::UserId)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop the index
        manager
            .drop_index(
                Index::drop()
                    .table(Runs::Table)
                    .name("idx_runs_user_id")
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        // Drop the foreign key
        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .table(Runs::Table)
                    .name("fk_runs_user_id")
                    .to_owned(),
            )
            .await?;

        // Drop the column
        manager
            .alter_table(
                Table::alter()
                    .table(Runs::Table)
                    .drop_column(Runs::UserId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Runs {
    Table,
    UserId,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
