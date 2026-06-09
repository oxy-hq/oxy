use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum MetricAnomalies {
    Table,
    Id,
    WorkspaceId,
    Measure,
    TimeDimension,
    Granularity,
    PeriodStart,
    PeriodEnd,
    Observed,
    Expected,
    LowerBound,
    UpperBound,
    ZScore,
    Severity,
    Status,
    Label,
    DetectedAt,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(MetricAnomalies::Table)
                    .if_not_exists()
                    .col(uuid(MetricAnomalies::Id).not_null().primary_key())
                    .col(uuid(MetricAnomalies::WorkspaceId).not_null())
                    .col(string(MetricAnomalies::Measure).not_null())
                    .col(string(MetricAnomalies::TimeDimension).not_null())
                    .col(string(MetricAnomalies::Granularity).not_null())
                    // Period the flagged observation covers (start inclusive, end exclusive).
                    .col(timestamp_with_time_zone(MetricAnomalies::PeriodStart).not_null())
                    .col(timestamp_with_time_zone(MetricAnomalies::PeriodEnd).not_null())
                    .col(double(MetricAnomalies::Observed).not_null())
                    .col(double(MetricAnomalies::Expected).not_null())
                    .col(double(MetricAnomalies::LowerBound).not_null())
                    .col(double(MetricAnomalies::UpperBound).not_null())
                    .col(double(MetricAnomalies::ZScore).not_null())
                    // "low" | "medium" | "high"
                    .col(string(MetricAnomalies::Severity).not_null())
                    // "new" | "acknowledged" | "dismissed"
                    .col(string(MetricAnomalies::Status).not_null().default("new"))
                    .col(string_null(MetricAnomalies::Label))
                    .col(
                        timestamp_with_time_zone(MetricAnomalies::DetectedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        timestamp_with_time_zone(MetricAnomalies::UpdatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // Inbox query: latest unresolved anomalies for a workspace.
        manager
            .create_index(
                Index::create()
                    .name("idx_metric_anomalies_workspace_status_detected")
                    .table(MetricAnomalies::Table)
                    .col(MetricAnomalies::WorkspaceId)
                    .col(MetricAnomalies::Status)
                    .col(MetricAnomalies::DetectedAt)
                    .to_owned(),
            )
            .await?;

        // De-dup guard: prevents re-inserting the same (workspace, measure,
        // time-dim, period_start) on a repeat scan. The scan helper does an
        // ON CONFLICT update instead of insert.
        manager
            .create_index(
                Index::create()
                    .name("uq_metric_anomalies_workspace_measure_period")
                    .table(MetricAnomalies::Table)
                    .col(MetricAnomalies::WorkspaceId)
                    .col(MetricAnomalies::Measure)
                    .col(MetricAnomalies::TimeDimension)
                    .col(MetricAnomalies::PeriodStart)
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
                    .table(MetricAnomalies::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
