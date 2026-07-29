//! Add `granularity` to the `metric_anomalies` unique index so a measure
//! monitored at two grains keeps two rows.
//!
//! `SegmentKey` — this crate's own definition of a segment — has always
//! included `granularity`, and the coverage table added in this same release
//! designs explicitly for a daily **and** a weekly monitor over one
//! `(measure, time_dimension)` pair. The anomaly row identity never caught up:
//! it keyed on (workspace_id, measure, time_dimension, dimension_key,
//! period_start), which is grain-blind.
//!
//! Those two grains collide whenever their buckets share a start instant — a
//! Monday daily bucket and the weekly bucket that opens on the same Monday. The
//! daily row is then found as the "existing" row for the weekly detection and
//! overwritten in place: one of the two anomalies silently stops existing, and
//! its `granularity` column flips to the other grain's value.
//!
//! Widening a unique index can never fail on existing data — every tuple that
//! was unique under the narrower key stays unique under the wider one — so this
//! needs no backfill or de-duplication step.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum MetricAnomalies {
    Table,
    Granularity,
    DimensionKey,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("uq_metric_anomalies_workspace_measure_period_dim")
                    .table(MetricAnomalies::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("uq_metric_anomalies_workspace_measure_period_dim_grain")
                    .table(MetricAnomalies::Table)
                    .col(Alias::new("workspace_id"))
                    .col(Alias::new("measure"))
                    .col(Alias::new("time_dimension"))
                    .col(MetricAnomalies::Granularity)
                    .col(Alias::new("period_start"))
                    .col(MetricAnomalies::DimensionKey)
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
                    .name("uq_metric_anomalies_workspace_measure_period_dim_grain")
                    .table(MetricAnomalies::Table)
                    .to_owned(),
            )
            .await?;

        // Narrowing back can fail if two grains have since filed rows that
        // share a period_start — that is the collision this migration exists to
        // allow, so a `down` on a workspace running both grains is expected to
        // require manual de-duplication first.
        manager
            .create_index(
                Index::create()
                    .name("uq_metric_anomalies_workspace_measure_period_dim")
                    .table(MetricAnomalies::Table)
                    .col(Alias::new("workspace_id"))
                    .col(Alias::new("measure"))
                    .col(Alias::new("time_dimension"))
                    .col(Alias::new("period_start"))
                    .col(MetricAnomalies::DimensionKey)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
