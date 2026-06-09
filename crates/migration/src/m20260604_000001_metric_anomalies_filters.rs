use sea_orm_migration::{prelude::*, schema::*};

/// Adds `dimension_key` (stable filter string for unique index) and `filters`
/// (JSON blob of raw monitor filters) to `metric_anomalies`.
///
/// The original unique index only covered (workspace_id, measure,
/// time_dimension, period_start) — that collides when two monitors for
/// different stores share the same measure + period. We drop it and replace it
/// with one that includes `dimension_key` so per-store and chain-wide
/// anomalies coexist correctly.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum MetricAnomalies {
    Table,
    DimensionKey,
    Filters,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add dimension_key — empty string for chain-wide monitors.
        manager
            .alter_table(
                Table::alter()
                    .table(MetricAnomalies::Table)
                    .add_column(string(MetricAnomalies::DimensionKey).not_null().default(""))
                    .to_owned(),
            )
            .await?;

        // Add filters — nullable JSON blob.
        manager
            .alter_table(
                Table::alter()
                    .table(MetricAnomalies::Table)
                    .add_column(json_binary_null(MetricAnomalies::Filters))
                    .to_owned(),
            )
            .await?;

        // Drop the old unique index (didn't include dimension_key).
        manager
            .drop_index(
                Index::drop()
                    .name("uq_metric_anomalies_workspace_measure_period")
                    .table(MetricAnomalies::Table)
                    .to_owned(),
            )
            .await?;

        // New unique index includes dimension_key so per-store anomalies
        // for the same period don't collide with chain-wide ones.
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

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
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
                    .name("uq_metric_anomalies_workspace_measure_period")
                    .table(MetricAnomalies::Table)
                    .col(Alias::new("workspace_id"))
                    .col(Alias::new("measure"))
                    .col(Alias::new("time_dimension"))
                    .col(Alias::new("period_start"))
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(MetricAnomalies::Table)
                    .drop_column(MetricAnomalies::Filters)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(MetricAnomalies::Table)
                    .drop_column(MetricAnomalies::DimensionKey)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
