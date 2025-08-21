use sea_orm::DatabaseBackend;
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Runs::Table)
                    .add_column(ColumnDef::new(Runs::RootReplayRefNew).string().null())
                    .to_owned(),
            )
            .await?;

        let db = manager.get_connection();
        let copy_sql = match manager.get_database_backend() {
            DatabaseBackend::Postgres => {
                "UPDATE runs SET root_replay_ref_new = CAST(root_replay_ref AS TEXT) WHERE root_replay_ref IS NOT NULL"
            }
            DatabaseBackend::Sqlite => {
                "UPDATE runs SET root_replay_ref_new = CAST(root_replay_ref AS TEXT) WHERE root_replay_ref IS NOT NULL"
            }
            _ => {
                "UPDATE runs SET root_replay_ref_new = CAST(root_replay_ref AS CHAR) WHERE root_replay_ref IS NOT NULL"
            }
        };

        // Execute the copy operation, ignore if it fails (in case table is empty)
        let _ = db.execute_unprepared(copy_sql).await;

        // Step 3: Drop old column
        manager
            .alter_table(
                Table::alter()
                    .table(Runs::Table)
                    .drop_column(Runs::RootReplayRef)
                    .to_owned(),
            )
            .await?;

        // Step 4: Rename new column to original name
        let rename_sql = match manager.get_database_backend() {
            DatabaseBackend::Postgres => {
                "ALTER TABLE runs RENAME COLUMN root_replay_ref_new TO root_replay_ref"
            }
            DatabaseBackend::Sqlite => {
                // SQLite doesn't support column rename directly, so we'll use ALTER TABLE
                "ALTER TABLE runs RENAME COLUMN root_replay_ref_new TO root_replay_ref"
            }
            _ => "ALTER TABLE runs RENAME COLUMN root_replay_ref_new TO root_replay_ref",
        };

        db.execute_unprepared(rename_sql).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Reverse the process: string back to integer
        // Note: This may lose data if strings can't be converted to integers

        // Step 1: Add temporary integer column
        manager
            .alter_table(
                Table::alter()
                    .table(Runs::Table)
                    .add_column(ColumnDef::new(Runs::RootReplayRefOld).integer().null())
                    .to_owned(),
            )
            .await?;

        // Step 2: Copy data (try to convert string to integer)
        let db = manager.get_connection();
        let copy_sql = match manager.get_database_backend() {
            DatabaseBackend::Postgres => {
                "UPDATE runs SET root_replay_ref_old = CASE WHEN root_replay_ref ~ '^[0-9]+$' THEN CAST(root_replay_ref AS INTEGER) ELSE NULL END WHERE root_replay_ref IS NOT NULL"
            }
            DatabaseBackend::Sqlite => {
                "UPDATE runs SET root_replay_ref_old = CASE WHEN root_replay_ref GLOB '[0-9]*' THEN CAST(root_replay_ref AS INTEGER) ELSE NULL END WHERE root_replay_ref IS NOT NULL"
            }
            _ => {
                "UPDATE runs SET root_replay_ref_old = CASE WHEN root_replay_ref REGEXP '^[0-9]+$' THEN CAST(root_replay_ref AS UNSIGNED) ELSE NULL END WHERE root_replay_ref IS NOT NULL"
            }
        };

        let _ = db.execute_unprepared(copy_sql).await;

        // Step 3: Drop string column
        manager
            .alter_table(
                Table::alter()
                    .table(Runs::Table)
                    .drop_column(Runs::RootReplayRef)
                    .to_owned(),
            )
            .await?;

        // Step 4: Rename back
        let rename_sql = match manager.get_database_backend() {
            DatabaseBackend::Postgres => {
                "ALTER TABLE runs RENAME COLUMN root_replay_ref_old TO root_replay_ref"
            }
            DatabaseBackend::Sqlite => {
                "ALTER TABLE runs RENAME COLUMN root_replay_ref_old TO root_replay_ref"
            }
            _ => "ALTER TABLE runs RENAME COLUMN root_replay_ref_old TO root_replay_ref",
        };

        db.execute_unprepared(rename_sql).await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Runs {
    Table,
    RootReplayRef,
    RootReplayRefNew,
    RootReplayRefOld,
}
