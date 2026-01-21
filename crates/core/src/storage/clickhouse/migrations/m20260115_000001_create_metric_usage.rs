use async_trait::async_trait;
use clickhouse::Client;

use oxy_shared::errors::OxyError;

use super::migrator::{ClickHouseMigration, Result};

pub struct CreateMetricUsage;

#[async_trait]
impl ClickHouseMigration for CreateMetricUsage {
    fn name(&self) -> &'static str {
        "create_metric_usage"
    }

    fn version(&self) -> &'static str {
        "20260115_000001"
    }

    async fn up(&self, client: &Client) -> Result<()> {
        // Create metric_usage table for tracking which metrics are queried most
        client
            .query(
                r#"
                CREATE TABLE IF NOT EXISTS metric_usage (
                    Id UUID DEFAULT generateUUIDv4(),

                    -- Metric identification
                    MetricName LowCardinality(String),

                    -- Context of usage
                    SourceType LowCardinality(String),
                    SourceRef LowCardinality(String),
                    Context String DEFAULT '',
                    ContextTypes String DEFAULT '[]',

                    -- Correlation
                    TraceId String,

                    -- Timestamps
                    CreatedAt DateTime64(3) DEFAULT now64(3)
                )
                ENGINE = MergeTree()
                PARTITION BY toYYYYMM(CreatedAt)
                ORDER BY (MetricName, SourceType, CreatedAt)
                "#,
            )
            .execute()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to create metric_usage: {e}")))?;

        Ok(())
    }

    async fn down(&self, client: &Client) -> Result<()> {
        client
            .query("DROP TABLE IF EXISTS metric_usage")
            .execute()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to drop metric_usage: {e}")))?;

        Ok(())
    }
}
