//! Add `monthly_vlm_budget_micros` to workspaces (IoT Phase 4c).
//!
//! NULL = no budget configured; the camera-fleet UI shows the
//! projected-cost banner only when a non-null budget is set. Using
//! `BIGINT` (i64) for USD micros lets a single workspace ceiling
//! cover ~$9.2T before overflow, which is fine.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Workspaces {
    Table,
    MonthlyVlmBudgetMicros,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .add_column(
                        ColumnDef::new(Workspaces::MonthlyVlmBudgetMicros)
                            .big_integer()
                            .null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .drop_column(Workspaces::MonthlyVlmBudgetMicros)
                    .to_owned(),
            )
            .await
    }
}
