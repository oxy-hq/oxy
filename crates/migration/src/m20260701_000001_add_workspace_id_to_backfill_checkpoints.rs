use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Scope checkpoints to the owning workspace so tenants sharing a relative
        // `pipeline_ref` path don't collide (coverage leak / resume skipping
        // across the tenant boundary). Additive over m20260630's create: that
        // migration was already applied in dev without this column, so the column
        // is ALTERed in here rather than by editing an already-applied migration.
        if !manager
            .has_column("backfill_checkpoints", "workspace_id")
            .await?
        {
            // Existing single-tenant local rows backfill to the nil workspace
            // (LOCAL_WORKSPACE_ID); the app always sets workspace_id explicitly
            // thereafter, so the default only covers the ALTER's existing rows.
            let nil_uuid = Expr::cust("'00000000-0000-0000-0000-000000000000'::uuid");
            manager
                .alter_table(
                    Table::alter()
                        .table(BackfillCheckpoints::Table)
                        .add_column(
                            uuid(BackfillCheckpoints::WorkspaceId)
                                .not_null()
                                .default(nil_uuid),
                        )
                        .to_owned(),
                )
                .await?;
        }

        // Re-scope the unique chunk index to lead with workspace_id, so two
        // workspaces can hold the same (pipeline_ref, period_start, period_end)
        // chunk without violating uniqueness.
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

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Restore the pre-workspace_id unique index, then drop the column.
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
                    .name("idx_backfill_checkpoints_chunk")
                    .table(BackfillCheckpoints::Table)
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
                    .drop_column(BackfillCheckpoints::WorkspaceId)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum BackfillCheckpoints {
    Table,
    WorkspaceId,
    PipelineRef,
    PeriodStart,
    PeriodEnd,
}
