use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ── backfill_checkpoints ──────────────────────────────────────────────
        // One row per (pipeline, period chunk) of a chunked backfill. Drives
        // resume (skip `done`) and coverage ("what period is missing?"). Generic
        // — any `*.airway.yml` whose source honours a [backfill_from, backfill_to)
        // window uses it.
        manager
            .create_table(
                Table::create()
                    .table(BackfillCheckpoints::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(BackfillCheckpoints::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(BackfillCheckpoints::PipelineRef)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BackfillCheckpoints::PeriodStart)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BackfillCheckpoints::PeriodEnd)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BackfillCheckpoints::Status)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(BackfillCheckpoints::RunId).string().null())
                    .col(
                        ColumnDef::new(BackfillCheckpoints::RowCount)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(BackfillCheckpoints::Attempts)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(BackfillCheckpoints::Error).text().null())
                    .col(
                        ColumnDef::new(BackfillCheckpoints::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(BackfillCheckpoints::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // One checkpoint per chunk — also the upsert target and the index that
        // makes "find the next not-done chunk" / coverage scans cheap.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_backfill_checkpoints_chunk")
                    .table(BackfillCheckpoints::Table)
                    .col(BackfillCheckpoints::PipelineRef)
                    .col(BackfillCheckpoints::PeriodStart)
                    .col(BackfillCheckpoints::PeriodEnd)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(BackfillCheckpoints::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum BackfillCheckpoints {
    Table,
    Id,
    PipelineRef,
    PeriodStart,
    PeriodEnd,
    Status,
    RunId,
    RowCount,
    Attempts,
    Error,
    CreatedAt,
    UpdatedAt,
}
