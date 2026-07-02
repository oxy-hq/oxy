use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ── backfill_ranges ───────────────────────────────────────────────────
        // One row per user-initiated backfill window; the parent of the chunks in
        // `backfill_checkpoints`. Keeps distinct backfills SEPARATE (per-run) so
        // overlapping windows never overwrite each other's chunks.
        manager
            .create_table(
                Table::create()
                    .table(BackfillRanges::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(BackfillRanges::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(BackfillRanges::WorkspaceId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BackfillRanges::PipelineRef)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BackfillRanges::RequestedFrom)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BackfillRanges::RequestedTo)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BackfillRanges::Granularity)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BackfillRanges::Concurrency)
                            .integer()
                            .not_null()
                            .default(4),
                    )
                    .col(ColumnDef::new(BackfillRanges::CreatedBy).uuid().null())
                    .col(ColumnDef::new(BackfillRanges::Status).string().not_null())
                    .col(
                        ColumnDef::new(BackfillRanges::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(BackfillRanges::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // List ranges for a pipeline (newest first) — the gantt query.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_backfill_ranges_pipeline")
                    .table(BackfillRanges::Table)
                    .col(BackfillRanges::WorkspaceId)
                    .col(BackfillRanges::PipelineRef)
                    .col(BackfillRanges::CreatedAt)
                    .to_owned(),
            )
            .await?;

        // ── backfill_checkpoints.backfill_range_id ────────────────────────────
        // Add nullable, back-fill, then flip to NOT NULL so existing rows survive.
        if !manager
            .has_column("backfill_checkpoints", "backfill_range_id")
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(BackfillCheckpoints::Table)
                        .add_column(uuid_null(BackfillCheckpoints::BackfillRangeId))
                        .to_owned(),
                )
                .await?;
        }

        // Back-compat: adopt every pre-existing checkpoint into one synthetic
        // "imported" range per (workspace, pipeline) so nothing is orphaned.
        let conn = manager.get_connection();
        conn.execute_unprepared(
            "INSERT INTO backfill_ranges \
               (id, workspace_id, pipeline_ref, requested_from, requested_to, \
                granularity, concurrency, created_by, status, created_at, updated_at) \
             SELECT gen_random_uuid(), workspace_id, pipeline_ref, \
                    MIN(period_start), MAX(period_end), 'day', 4, NULL, \
                    CASE \
                      WHEN bool_or(status IN ('pending','running')) THEN 'running' \
                      WHEN bool_and(status = 'done') THEN 'done' \
                      WHEN bool_or(status IN ('failed','timed_out','cancelled')) THEN 'failed' \
                      ELSE 'degraded' \
                    END, \
                    MIN(created_at), MAX(updated_at) \
             FROM backfill_checkpoints \
             WHERE backfill_range_id IS NULL \
             GROUP BY workspace_id, pipeline_ref",
        )
        .await?;
        conn.execute_unprepared(
            "UPDATE backfill_checkpoints c \
             SET backfill_range_id = r.id \
             FROM backfill_ranges r \
             WHERE c.backfill_range_id IS NULL \
               AND c.workspace_id = r.workspace_id \
               AND c.pipeline_ref = r.pipeline_ref",
        )
        .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(BackfillCheckpoints::Table)
                    .modify_column(uuid(BackfillCheckpoints::BackfillRangeId).not_null())
                    .to_owned(),
            )
            .await?;

        // Re-scope the unique chunk index to the owning range: the same period may
        // now exist under multiple ranges (per-run), unique only within a range.
        manager
            .drop_index(
                Index::drop()
                    .name("idx_backfill_checkpoints_chunk")
                    .table(BackfillCheckpoints::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_backfill_checkpoints_range_chunk")
                    .table(BackfillCheckpoints::Table)
                    .col(BackfillCheckpoints::BackfillRangeId)
                    .col(BackfillCheckpoints::PeriodStart)
                    .col(BackfillCheckpoints::PeriodEnd)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // FK so dropping a range cascades its chunks away.
        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk_backfill_checkpoints_range")
                    .from(
                        BackfillCheckpoints::Table,
                        BackfillCheckpoints::BackfillRangeId,
                    )
                    .to(BackfillRanges::Table, BackfillRanges::Id)
                    .on_delete(ForeignKeyAction::Cascade)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .name("fk_backfill_checkpoints_range")
                    .table(BackfillCheckpoints::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_backfill_checkpoints_range_chunk")
                    .table(BackfillCheckpoints::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_backfill_checkpoints_chunk")
                    .table(BackfillCheckpoints::Table)
                    .col(BackfillCheckpoints::WorkspaceId)
                    .col(BackfillCheckpoints::PipelineRef)
                    .col(BackfillCheckpoints::PeriodStart)
                    .col(BackfillCheckpoints::PeriodEnd)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(BackfillCheckpoints::Table)
                    .drop_column(BackfillCheckpoints::BackfillRangeId)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(BackfillRanges::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum BackfillRanges {
    Table,
    Id,
    WorkspaceId,
    PipelineRef,
    RequestedFrom,
    RequestedTo,
    Granularity,
    Concurrency,
    CreatedBy,
    Status,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum BackfillCheckpoints {
    Table,
    BackfillRangeId,
    WorkspaceId,
    PipelineRef,
    PeriodStart,
    PeriodEnd,
}
