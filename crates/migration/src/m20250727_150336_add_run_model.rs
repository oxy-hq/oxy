use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create the Runs table
        manager
            .create_table(
                Table::create()
                    .table(Runs::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Runs::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Runs::SourceId).string().not_null())
                    .col(ColumnDef::new(Runs::RunIndex).integer().null())
                    .col(ColumnDef::new(Runs::RootSourceId).string().null())
                    .col(ColumnDef::new(Runs::RootRunIndex).integer().null())
                    .col(ColumnDef::new(Runs::RootReplayRef).integer().null())
                    .col(ColumnDef::new(Runs::Metadata).json().null())
                    .col(ColumnDef::new(Runs::Blocks).json().null())
                    .col(ColumnDef::new(Runs::Children).json().null())
                    .col(ColumnDef::new(Runs::Error).string().null())
                    .col(
                        ColumnDef::new(Runs::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Runs::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_unique_source_runindex")
                    .table(Runs::Table)
                    .col(Runs::SourceId)
                    .col(Runs::RunIndex)
                    .cond_where(Expr::col(Runs::RunIndex).is_not_null())
                    .unique()
                    .to_owned(),
            )
            .await?;
        // Create the Checkpoints table
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
                    // Foreign key to Runs table
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

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_unique_run_replay_checkpoint")
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(Checkpoints::Table).to_owned())
            .await?;
        manager
            .drop_index(Index::drop().name("idx_unique_source_runindex").to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Runs::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum Runs {
    Table,
    Id,
    SourceId,
    RunIndex,
    RootSourceId,
    RootRunIndex,
    RootReplayRef,
    Metadata,
    Blocks,
    Children,
    Error,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
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
