//! Drop the retired `checkpoints` table.
//!
//! The pre-agentic executor's checkpoint feature was removed in the old-executor
//! retirement (Phase 4a, #3043): its Rust code and the `checkpoints` Sea-ORM entity
//! were deleted then, leaving the table orphaned — no reader, no writer. This drops
//! the table itself. Retry/replay now flows through the agentic run path.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Checkpoints::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Faithful reverse of m20250727_150336_add_run_model's checkpoints create
        // (columns, FK to `runs`, unique index). The table is retired, so this path
        // exists only to keep the migration reversible.
        manager
            .create_table(
                Table::create()
                    .table(Checkpoints::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Checkpoints::Id).uuid().not_null())
                    .col(ColumnDef::new(Checkpoints::RunId).uuid().not_null())
                    .col(ColumnDef::new(Checkpoints::ReplayId).string().not_null())
                    .col(
                        ColumnDef::new(Checkpoints::CheckpointHash)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Checkpoints::Output).json().null())
                    .col(ColumnDef::new(Checkpoints::Events).json().null())
                    .col(ColumnDef::new(Checkpoints::ChildRunInfo).json().null())
                    .col(ColumnDef::new(Checkpoints::LoopValues).json().null())
                    .col(
                        ColumnDef::new(Checkpoints::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Checkpoints::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_checkpoints_run_id")
                            .from(Checkpoints::Table, Checkpoints::RunId)
                            .to(Runs::Table, Runs::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_unique_run_replay_checkpoint")
                    .table(Checkpoints::Table)
                    .col(Checkpoints::RunId)
                    .col(Checkpoints::ReplayId)
                    .unique()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Checkpoints {
    Table,
    Id,
    RunId,
    ReplayId,
    CheckpointHash,
    Output,
    Events,
    ChildRunInfo,
    LoopValues,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Runs {
    Table,
    Id,
}
