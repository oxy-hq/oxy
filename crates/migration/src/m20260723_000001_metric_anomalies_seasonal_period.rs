use sea_orm_migration::prelude::*;

/// Adds `seasonal_period` to `metric_anomalies` — the dominant seasonal cycle
/// length (in units of the row's `granularity`) taken from the monitor's
/// detection config at scan time.
///
/// Root-cause explains diff the anomalous bucket against the same phase one
/// seasonal cycle earlier (same weekday last week for a daily/weekly-seasonal
/// monitor). Without this column the explain path could not see the detection
/// config and hardcoded the comparison offset, so a daily monitor with a
/// weekly seasonality was still compared against the immediately-preceding day
/// (the weekend). Snapshotting the period here keeps explain a pure
/// persisted-data read — no workspace-FS re-read at request time.
///
/// Nullable + additive: existing rows stay `NULL` and the explain path falls
/// back to the granularity default (matching the detector's own defaults);
/// the next scan backfills the value on upsert.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE metric_anomalies \
                   ADD COLUMN IF NOT EXISTS seasonal_period INTEGER",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE metric_anomalies \
                   DROP COLUMN IF EXISTS seasonal_period",
            )
            .await?;
        Ok(())
    }
}
