use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// A counter table that hands out monotonically increasing run indices per
/// (project_id, branch_id, source_id) tuple.  The atomic upsert
///
///   INSERT ... ON CONFLICT DO UPDATE SET last_value = last_value + 1 RETURNING last_value
///
/// replaces both the PostgreSQL advisory lock and the SELECT ... FOR UPDATE
/// approach, and works correctly across multiple server processes.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(RunSequences::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(RunSequences::ProjectId).uuid().not_null())
                    .col(ColumnDef::new(RunSequences::BranchId).uuid().not_null())
                    .col(ColumnDef::new(RunSequences::SourceId).string().not_null())
                    .col(
                        ColumnDef::new(RunSequences::LastValue)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .primary_key(
                        Index::create()
                            .col(RunSequences::ProjectId)
                            .col(RunSequences::BranchId)
                            .col(RunSequences::SourceId),
                    )
                    .to_owned(),
            )
            .await?;

        // Seed sequences from existing runs so the counter starts above any
        // run_index already in the table, preventing unique constraint violations.
        manager
            .get_connection()
            .execute_unprepared(
                "INSERT INTO run_sequences (project_id, branch_id, source_id, last_value) \
                 SELECT project_id, branch_id, source_id, MAX(run_index) \
                 FROM runs \
                 WHERE project_id IS NOT NULL \
                   AND branch_id IS NOT NULL \
                   AND source_id IS NOT NULL \
                   AND run_index IS NOT NULL \
                 GROUP BY project_id, branch_id, source_id \
                 ON CONFLICT (project_id, branch_id, source_id) \
                 DO UPDATE SET last_value = EXCLUDED.last_value",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RunSequences::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum RunSequences {
    Table,
    ProjectId,
    BranchId,
    SourceId,
    LastValue,
}
