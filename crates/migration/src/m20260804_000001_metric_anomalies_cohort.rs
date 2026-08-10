//! Group *simultaneous* anomalies across segments into a single **cohort**.
//!
//! Orthogonal to `event_id`, which chains consecutive buckets *within* one
//! segment. Nothing until now grouped across segments at a point in time, so a
//! chain-wide collapse — 21 of 21 stores below their own Saturday median on the
//! same day — filed 21 separate rows saying the same thing, and the one store
//! that fell twice as far as the rest was buried among them.
//!
//! Two columns, not one: `cohort_id` is the shared identity, and
//! `cohort_deviation` ranks a member against its own cluster (1.0 is a typical
//! member, well below 1.0 is the row worth acting on). Overloading `event_id`
//! for this was rejected — a two-day chain-wide slide is both an event and a
//! cohort, and one column cannot represent it.
//!
//! Both nullable with no default: a row with no cohort is the common case, and
//! `NULL` says "not part of a cluster" without a sentinel. Rows detected before
//! this existed have no cohort, and backfilling would mean inventing clusters
//! from scans that never computed a share.

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum MetricAnomalies {
    Table,
    WorkspaceId,
    CohortId,
    CohortDeviation,
    CohortLabel,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(MetricAnomalies::Table)
                    .add_column(uuid_null(MetricAnomalies::CohortId))
                    .add_column(double_null(MetricAnomalies::CohortDeviation))
                    .add_column(text_null(MetricAnomalies::CohortLabel))
                    .to_owned(),
            )
            .await?;

        // The inbox groups by cohort within a workspace.
        manager
            .create_index(
                Index::create()
                    .name("idx_metric_anomalies_workspace_cohort")
                    .table(MetricAnomalies::Table)
                    .col(MetricAnomalies::WorkspaceId)
                    .col(MetricAnomalies::CohortId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_metric_anomalies_workspace_cohort")
                    .table(MetricAnomalies::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(MetricAnomalies::Table)
                    .drop_column(MetricAnomalies::CohortId)
                    .drop_column(MetricAnomalies::CohortDeviation)
                    .drop_column(MetricAnomalies::CohortLabel)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
