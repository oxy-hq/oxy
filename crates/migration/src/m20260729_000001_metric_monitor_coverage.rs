//! Per-segment scan coverage, so a monitor that is not scoring can say so.
//!
//! `scan_one` skips a series with too little history and returns `Ok(vec![])`
//! — correctly kept out of `monitors_failed`, since a warming-up segment is
//! not a failure. But that left the Monitors tab unable to tell "healthy, no
//! anomalies" from "not scoring at all": an operator reading an empty inbox
//! would conclude the metric was fine. One row per scanned segment records how
//! much history it actually has against how much it needs.

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum MetricMonitorCoverage {
    Table,
    Id,
    WorkspaceId,
    Measure,
    TimeDimension,
    Granularity,
    DimensionKey,
    Filters,
    Label,
    MeasuredBuckets,
    RequiredBuckets,
    LastScannedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(MetricMonitorCoverage::Table)
                    .if_not_exists()
                    .col(uuid(MetricMonitorCoverage::Id).not_null().primary_key())
                    .col(uuid(MetricMonitorCoverage::WorkspaceId).not_null())
                    .col(string(MetricMonitorCoverage::Measure).not_null())
                    .col(string(MetricMonitorCoverage::TimeDimension).not_null())
                    .col(string(MetricMonitorCoverage::Granularity).not_null())
                    // Empty string for chain-wide monitors, matching
                    // `metric_anomalies.dimension_key`.
                    .col(
                        string(MetricMonitorCoverage::DimensionKey)
                            .not_null()
                            .default(""),
                    )
                    .col(json_binary_null(MetricMonitorCoverage::Filters))
                    .col(string_null(MetricMonitorCoverage::Label))
                    // Buckets the warehouse actually returned, and the
                    // statistical floor (`gates::min_history_buckets`) they are
                    // measured against. Scoring happens iff measured >= required.
                    .col(integer(MetricMonitorCoverage::MeasuredBuckets).not_null())
                    .col(integer(MetricMonitorCoverage::RequiredBuckets).not_null())
                    .col(
                        timestamp_with_time_zone(MetricMonitorCoverage::LastScannedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // One row per segment; a repeat scan updates it in place.
        //
        // `granularity` is part of the key, unlike in `metric_anomalies` where
        // `period_start` already separates grains. Coverage has no such column,
        // so without it a daily and a weekly monitor over the same measure and
        // time dimension would share one row and overwrite each other's counts
        // — and their floors differ (56 daily buckets vs 32 weekly at the default
        // seasonality), so the
        // survivor would be measured against the wrong one.
        manager
            .create_index(
                Index::create()
                    .name("uq_metric_monitor_coverage_workspace_measure_gran_dim")
                    .table(MetricMonitorCoverage::Table)
                    .col(MetricMonitorCoverage::WorkspaceId)
                    .col(MetricMonitorCoverage::Measure)
                    .col(MetricMonitorCoverage::TimeDimension)
                    .col(MetricMonitorCoverage::Granularity)
                    .col(MetricMonitorCoverage::DimensionKey)
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
                    .table(MetricMonitorCoverage::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
