use async_trait::async_trait;
use clickhouse::Client;

use oxy_shared::errors::OxyError;

use super::migrator::{ClickHouseMigration, Result};

pub struct AddSourceFieldsToIntentClassifications;

#[async_trait]
impl ClickHouseMigration for AddSourceFieldsToIntentClassifications {
    fn name(&self) -> &'static str {
        "add_source_fields_to_intent_classifications"
    }

    fn version(&self) -> &'static str {
        "20260112_000001"
    }

    async fn up(&self, client: &Client) -> Result<()> {
        // Add SourceType column (e.g., "agent", "api", "cli")
        client
            .query(
                r#"
                ALTER TABLE intent_classifications
                ADD COLUMN IF NOT EXISTS SourceType LowCardinality(String) DEFAULT 'agent'
                "#,
            )
            .execute()
            .await
            .map_err(|e| {
                oxy_shared::errors::OxyError::RuntimeError(format!(
                    "Failed to add SourceType column: {e}"
                ))
            })?;

        // Add Source column (the actual identifier, e.g., agent ref)
        client
            .query(
                r#"
                ALTER TABLE intent_classifications
                ADD COLUMN IF NOT EXISTS Source LowCardinality(String) DEFAULT ''
                "#,
            )
            .execute()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to add Source column: {e}")))?;

        Ok(())
    }

    async fn down(&self, client: &Client) -> Result<()> {
        // Remove Source column
        client
            .query(
                r#"
                ALTER TABLE intent_classifications
                DROP COLUMN IF EXISTS Source
                "#,
            )
            .execute()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to drop Source column: {e}")))?;

        // Remove SourceType column
        client
            .query(
                r#"
                ALTER TABLE intent_classifications
                DROP COLUMN IF EXISTS SourceType
                "#,
            )
            .execute()
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to drop SourceType column: {e}"))
            })?;

        Ok(())
    }
}
