use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            -- Compiled `schemas/*.sql` — the DDL an Oxy engineer authors for a
            -- workspace, carried across the compile boundary like every other
            -- workspace artifact rather than read from disk at apply time.
            CREATE TABLE schema_migration_definitions (
                revision_id     UUID NOT NULL
                    REFERENCES revisions(revision_id) ON DELETE CASCADE,
                file_path       TEXT NOT NULL,
                content_sha256  TEXT NOT NULL,
                content         TEXT NOT NULL,
                PRIMARY KEY (revision_id, file_path)
            );
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS schema_migration_definitions CASCADE")
            .await?;
        Ok(())
    }
}
