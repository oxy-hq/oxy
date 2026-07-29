//! Group consecutive flagged buckets of one segment into a single **event**.
//!
//! A sustained anomaly — a labour surge running Monday, Wednesday, Thursday —
//! files one row per bucket, so the inbox reports the same event three times
//! and "how many problems do I have" is unanswerable by counting rows.
//!
//! This adds an identity rather than merging the rows. One row per bucket is
//! kept deliberately: `explain_anomaly` compares a bucket against the same
//! seasonal phase one cycle back and would silently describe only the first day
//! of a merged range. Grouping therefore happens on read, keyed by this column,
//! and the explain contract is untouched.
//!
//! Nullable: rows detected before this existed have no event, and backfilling
//! one would mean inventing runs from history the scanner never linked.

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum MetricAnomalies {
    Table,
    WorkspaceId,
    EventId,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(MetricAnomalies::Table)
                    .add_column(uuid_null(MetricAnomalies::EventId))
                    .to_owned(),
            )
            .await?;

        // The inbox groups by event within a workspace.
        manager
            .create_index(
                Index::create()
                    .name("idx_metric_anomalies_workspace_event")
                    .table(MetricAnomalies::Table)
                    .col(MetricAnomalies::WorkspaceId)
                    .col(MetricAnomalies::EventId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_metric_anomalies_workspace_event")
                    .table(MetricAnomalies::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(MetricAnomalies::Table)
                    .drop_column(MetricAnomalies::EventId)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
