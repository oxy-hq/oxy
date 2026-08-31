use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ── world_model_events ────────────────────────────────────────────────
        // Durable carrier for the world-model live feed. The broadcast bus in
        // `world_model.rs` is per-process, so a webhook that lands on a serve
        // replica never reaches a viewer subscribed on the ide. Publishers
        // append here; every pod tails by `id` and fans rows onto its own bus.
        //
        // `id` is a bigserial on purpose: the tailer's cursor has to be
        // monotonic and cheap to range-scan. A timestamp cursor would drop
        // rows whose transaction committed out of order.
        manager
            .create_table(
                Table::create()
                    .table(WorldModelEvents::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WorldModelEvents::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(WorldModelEvents::WorkspaceId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorldModelEvents::Payload)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorldModelEvents::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // Backfill for a viewer that connects mid-shift: newest-first within one
        // workspace. Also what an `orders/min` count reads.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_world_model_events_workspace_id_desc")
                    .table(WorldModelEvents::Table)
                    .col(WorldModelEvents::WorkspaceId)
                    .col((WorldModelEvents::Id, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        // The reaper trims by age; without this it degrades to a seq scan once
        // the retained window gets big.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_world_model_events_created_at")
                    .table(WorldModelEvents::Table)
                    .col(WorldModelEvents::CreatedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(WorldModelEvents::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum WorldModelEvents {
    Table,
    Id,
    WorkspaceId,
    Payload,
    CreatedAt,
}
