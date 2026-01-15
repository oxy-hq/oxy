//! ClickHouse storage module for observability data
//!
//! This module provides a unified ClickHouse client for:
//! - Trace storage and querying (OpenTelemetry)
//! - Intent classification storage
//! - ClickHouse-specific migrations
//!
//! ## Environment Variables
//!
//! Configure the observability ClickHouse connection using:
//! - `OXY_CLICKHOUSE_URL` - ClickHouse HTTP endpoint (default: http://localhost:8123)
//! - `OXY_CLICKHOUSE_USER` - Database user (default: default)
//! - `OXY_CLICKHOUSE_PASSWORD` - Database password (default: empty)
//! - `OXY_CLICKHOUSE_DATABASE` - Database name (default: otel)

pub mod migrations;

use clickhouse::Client;

use oxy_shared::errors::OxyError;

// Environment variable names for observability ClickHouse
const ENV_CLICKHOUSE_URL: &str = "OXY_CLICKHOUSE_URL";
const ENV_CLICKHOUSE_USER: &str = "OXY_CLICKHOUSE_USER";
const ENV_CLICKHOUSE_PASSWORD: &str = "OXY_CLICKHOUSE_PASSWORD";
const ENV_CLICKHOUSE_DATABASE: &str = "OXY_CLICKHOUSE_DATABASE";

/// Configuration for ClickHouse connection (observability/OTEL data)
#[derive(Debug, Clone)]
pub struct ClickHouseConfig {
    /// ClickHouse HTTP endpoint URL
    pub url: String,
    /// Database user
    pub user: String,
    /// Database password
    pub password: String,
    /// Database name (default: "otel")
    pub database: String,
}

impl Default for ClickHouseConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:8123".to_string(),
            user: "default".to_string(),
            password: String::new(),
            database: "otel".to_string(),
        }
    }
}

impl ClickHouseConfig {
    /// Create config from environment variables
    pub fn from_env() -> Self {
        Self {
            url: std::env::var(ENV_CLICKHOUSE_URL)
                .unwrap_or_else(|_| "http://localhost:8123".to_string()),
            user: std::env::var(ENV_CLICKHOUSE_USER).unwrap_or_else(|_| "default".to_string()),
            password: std::env::var(ENV_CLICKHOUSE_PASSWORD)
                .unwrap_or_else(|_| "default".to_string()),
            database: std::env::var(ENV_CLICKHOUSE_DATABASE).unwrap_or_else(|_| "otel".to_string()),
        }
    }

    /// Check if ClickHouse is configured via environment variables
    pub fn is_configured() -> bool {
        std::env::var(ENV_CLICKHOUSE_URL).is_ok()
            || std::env::var(ENV_CLICKHOUSE_USER).is_ok()
            || std::env::var(ENV_CLICKHOUSE_PASSWORD).is_ok()
            || std::env::var(ENV_CLICKHOUSE_DATABASE).is_ok()
    }
}

/// Unified ClickHouse storage client for observability data
pub struct ClickHouseStorage {
    client: Client,
    #[allow(dead_code)]
    config: ClickHouseConfig,
}

impl std::fmt::Debug for ClickHouseStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClickHouseStorage")
            .field("database", &self.config.database)
            .field("url", &self.config.url)
            .finish_non_exhaustive()
    }
}

impl ClickHouseStorage {
    /// Create a new ClickHouse storage client
    pub fn new(config: ClickHouseConfig) -> Self {
        let client = Client::default()
            .with_url(&config.url)
            .with_user(&config.user)
            .with_password(&config.password)
            .with_database(&config.database);

        Self { client, config }
    }

    /// Create a new ClickHouse storage client from environment variables
    pub fn from_env() -> Self {
        Self::new(ClickHouseConfig::from_env())
    }

    /// Get a reference to the underlying ClickHouse client
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Run all ClickHouse migrations
    pub async fn run_migrations(&self) -> Result<(), OxyError> {
        let migrator = migrations::get_clickhouse_migrator();
        migrator.migrate_up(&self.client).await
    }

    /// Execute a raw query that returns no results
    pub async fn execute(&self, query: &str) -> Result<(), OxyError> {
        self.client
            .query(query)
            .execute()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("ClickHouse query execution failed: {e}")))
    }

    /// Check if a table exists in the database
    pub async fn table_exists(&self, table_name: &str) -> Result<bool, OxyError> {
        #[derive(clickhouse::Row, serde::Deserialize)]
        struct CountResult {
            #[serde(rename = "count()")]
            count: u64,
        }

        let query = format!(
            "SELECT count() FROM system.tables WHERE database = '{}' AND name = '{}'",
            self.config.database, table_name
        );

        let result: CountResult =
            self.client.query(&query).fetch_one().await.map_err(|e| {
                OxyError::RuntimeError(format!("Failed to check table existence: {e}"))
            })?;

        Ok(result.count > 0)
    }
}
