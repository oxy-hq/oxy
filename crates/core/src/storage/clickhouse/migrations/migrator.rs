//! ClickHouse migration system for observability data
//!
//! Since SeaORM doesn't support ClickHouse, we implement a custom migration
//! system that follows similar patterns to SeaORM migrations.

use async_trait::async_trait;
use clickhouse::Client;
use std::collections::HashMap;

use crate::errors::OxyError;

pub type Result<T> = std::result::Result<T, OxyError>;

#[async_trait]
pub trait ClickHouseMigration: Send + Sync {
    /// Migration name/identifier
    fn name(&self) -> &'static str;

    /// Migration version (timestamp format: YYYYMMDD_HHMMSS)
    fn version(&self) -> &'static str;

    /// Apply the migration
    async fn up(&self, client: &Client) -> Result<()>;

    /// Rollback the migration (optional)
    async fn down(&self, client: &Client) -> Result<()> {
        let _ = client;
        Err(OxyError::RuntimeError(format!(
            "Rollback not implemented for migration {}",
            self.name()
        )))
    }
}

pub struct ClickHouseMigrator {
    migrations: Vec<Box<dyn ClickHouseMigration>>,
}

impl ClickHouseMigrator {
    pub fn new() -> Self {
        Self {
            migrations: Vec::new(),
        }
    }

    pub fn add_migration(mut self, migration: Box<dyn ClickHouseMigration>) -> Self {
        self.migrations.push(migration);
        self
    }

    /// Initialize the migration tracking table
    pub async fn init_migration_table(&self, client: &Client) -> Result<()> {
        client
            .query(
                r#"
                CREATE TABLE IF NOT EXISTS __clickhouse_migrations (
                    version String,
                    name String,
                    applied_at DateTime64(9) DEFAULT now64(9)
                ) ENGINE = MergeTree()
                ORDER BY version
                "#,
            )
            .execute()
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to create migration table: {e}"))
            })?;

        Ok(())
    }

    /// Get applied migrations
    pub async fn get_applied_migrations(&self, client: &Client) -> Result<HashMap<String, String>> {
        #[derive(clickhouse::Row, serde::Deserialize)]
        struct MigrationRow {
            version: String,
            name: String,
        }

        let rows: Vec<MigrationRow> = client
            .query("SELECT version, name FROM __clickhouse_migrations ORDER BY version")
            .fetch_all()
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to fetch applied migrations: {e}"))
            })?;

        Ok(rows.into_iter().map(|r| (r.version, r.name)).collect())
    }

    /// Run pending migrations
    pub async fn migrate_up(&self, client: &Client) -> Result<()> {
        self.init_migration_table(client).await?;
        let applied = self.get_applied_migrations(client).await?;

        tracing::debug!("Applied migrations: {:?}", applied);

        // Sort migrations by version
        let mut migrations = self.migrations.iter().collect::<Vec<_>>();
        migrations.sort_by(|a, b| a.version().cmp(b.version()));

        tracing::debug!(
            "Sorted migrations: {:?}",
            migrations.iter().map(|m| m.version()).collect::<Vec<_>>()
        );

        for migration in migrations {
            if !applied.contains_key(migration.version()) {
                tracing::info!(
                    "Applying ClickHouse migration: {} ({})",
                    migration.name(),
                    migration.version()
                );

                // Apply migration
                migration.up(client).await?;

                // Record in migration table
                client
                    .query(&format!(
                        "INSERT INTO __clickhouse_migrations (version, name) VALUES ('{}', '{}')",
                        migration.version(),
                        migration.name().replace('\'', "\\'")
                    ))
                    .execute()
                    .await
                    .map_err(|e| {
                        OxyError::RuntimeError(format!("Failed to record migration: {e}"))
                    })?;

                tracing::info!("Applied ClickHouse migration: {}", migration.name());
            }
        }

        Ok(())
    }
}

impl Default for ClickHouseMigrator {
    fn default() -> Self {
        Self::new()
    }
}
