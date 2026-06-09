use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum MetricAnomalies {
    Table,
    ExplainCache,
    ExplainCachedAt,
}

/// Add a per-row cache of the explain decomposition so the Insights inbox
/// drawer survives page refreshes without re-running a 20-30s recursive
/// search. JSONB so we can store airlayer's full `ExplainResult` shape;
/// nullable so existing rows stay valid.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(MetricAnomalies::Table)
                    .add_column(json_binary_null(MetricAnomalies::ExplainCache))
                    .add_column(timestamp_with_time_zone_null(
                        MetricAnomalies::ExplainCachedAt,
                    ))
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(MetricAnomalies::Table)
                    .drop_column(MetricAnomalies::ExplainCache)
                    .drop_column(MetricAnomalies::ExplainCachedAt)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
