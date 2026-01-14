use async_trait::async_trait;
use clickhouse::Client;

use super::migrator::{ClickHouseMigration, Result};

pub struct CreateIntentTables;

#[async_trait]
impl ClickHouseMigration for CreateIntentTables {
    fn name(&self) -> &'static str {
        "create_intent_tables"
    }

    fn version(&self) -> &'static str {
        "20251228_000001"
    }

    async fn up(&self, client: &Client) -> Result<()> {
        // Create intent_clusters table
        client
            .query(
                r#"
                CREATE TABLE IF NOT EXISTS intent_clusters (
                    ClusterId UInt32,
                    IntentName LowCardinality(String),
                    IntentDescription String,
                    Centroid Array(Float32),
                    SampleQuestions Array(String),
                    QuestionCount UInt64,
                    CreatedAt DateTime64(9) DEFAULT now64(9),
                    UpdatedAt DateTime64(9) DEFAULT now64(9)
                ) ENGINE = ReplacingMergeTree(UpdatedAt)
                ORDER BY (ClusterId)
                "#,
            )
            .execute()
            .await
            .map_err(|e| {
                crate::errors::OxyError::RuntimeError(format!(
                    "Failed to create intent_clusters: {e}"
                ))
            })?;

        // Create intent_classifications table
        client
            .query(
                r#"
                CREATE TABLE IF NOT EXISTS intent_classifications (
                    TraceId String,
                    Question String,
                    ClusterId UInt32,
                    IntentName LowCardinality(String),
                    Confidence Float32,
                    Embedding Array(Float32),
                    ClassifiedAt DateTime64(9) DEFAULT now64(9)
                ) ENGINE = MergeTree()
                ORDER BY (TraceId, ClassifiedAt)
                "#,
            )
            .execute()
            .await
            .map_err(|e| {
                crate::errors::OxyError::RuntimeError(format!(
                    "Failed to create intent_classifications: {e}"
                ))
            })?;

        Ok(())
    }

    async fn down(&self, client: &Client) -> Result<()> {
        client
            .query("DROP TABLE IF EXISTS intent_classifications")
            .execute()
            .await
            .map_err(|e| {
                crate::errors::OxyError::RuntimeError(format!(
                    "Failed to drop intent_classifications: {e}"
                ))
            })?;

        client
            .query("DROP TABLE IF EXISTS intent_clusters")
            .execute()
            .await
            .map_err(|e| {
                crate::errors::OxyError::RuntimeError(format!(
                    "Failed to drop intent_clusters: {e}"
                ))
            })?;

        Ok(())
    }
}
