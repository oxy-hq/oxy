//! Database connector trait, configuration, and backend implementations.
//!
//! # Backends
//!
//! | Feature      | Connector               | Crate                  |
//! |--------------|-------------------------|------------------------|
//! | `duckdb`     | [`DuckDbConnector`]     | `duckdb`               |
//! | `postgres`   | [`PostgresConnector`]   | `tokio-postgres`       |
//! | `clickhouse` | [`ClickHouseConnector`] | `reqwest` (HTTP API)   |
//! | `snowflake`  | [`SnowflakeConnector`]  | `snowflake-api`        |
//! | `bigquery`   | [`BigQueryConnector`]   | `gcp-bigquery-client`  |

pub mod config;
pub mod connector;

#[cfg(feature = "duckdb")]
pub mod duckdb;

#[cfg(feature = "postgres")]
pub mod postgres;

#[cfg(feature = "clickhouse")]
pub mod clickhouse;

#[cfg(feature = "snowflake")]
pub mod snowflake;

#[cfg(feature = "bigquery")]
pub mod bigquery;

// ── Config re-exports ─────────────────────────────────────────────────────────

pub use config::{
    BigQueryConfig, ClickHouseConfig, ConnectorConfig, DuckDbConfig, DuckDbLoadStrategy,
    PostgresConfig, SnowflakeConfig,
};

// ── Trait re-exports ──────────────────────────────────────────────────────────

pub use connector::{
    ColumnStats, ConnectorError, DatabaseConnector, ExecutionResult, ResultSummary,
    SchemaColumnInfo, SchemaInfo, SchemaTableInfo, SqlDialect,
};

// ── Connector re-exports ──────────────────────────────────────────────────────

#[cfg(feature = "duckdb")]
pub use duckdb::{DuckDbConnection, DuckDbConnector, LoadStrategy, TableInfo, TableSource};

#[cfg(feature = "postgres")]
pub use postgres::PostgresConnector;

#[cfg(feature = "clickhouse")]
pub use clickhouse::ClickHouseConnector;

#[cfg(feature = "snowflake")]
pub use snowflake::SnowflakeConnector;

#[cfg(feature = "bigquery")]
pub use bigquery::BigQueryConnector;

// ── build_connector ───────────────────────────────────────────────────────────

/// Construct a `Box<dyn DatabaseConnector>` from a sync-compatible config.
///
/// Only [`ConnectorConfig::DuckDb`] is supported here (DuckDB opens synchronously).
/// For all other variants use [`build_connector_async`].
///
/// Returns `Err(ConnectorError::ConnectionError)` if the requested backend is
/// not compiled in (missing feature flag) or if the backend itself fails to
/// initialise.
pub fn build_connector(cfg: ConnectorConfig) -> Result<Box<dyn DatabaseConnector>, ConnectorError> {
    match cfg {
        ConnectorConfig::DuckDb(c) => {
            #[cfg(feature = "duckdb")]
            {
                use crate::duckdb::{DuckDbConnector, LoadStrategy};

                let strategy = match c.load_strategy {
                    DuckDbLoadStrategy::View => LoadStrategy::View,
                    DuckDbLoadStrategy::Materialized => LoadStrategy::Materialized,
                };
                let connector = DuckDbConnector::from_directory(&c.data_dir, strategy)
                    .map_err(|e| ConnectorError::ConnectionError(e.to_string()))?;
                Ok(Box::new(connector))
            }
            #[cfg(not(feature = "duckdb"))]
            {
                let _ = c;
                Err(ConnectorError::ConnectionError(
                    "DuckDB support is not compiled in — enable the 'duckdb' feature on \
                     agentic-connector"
                        .into(),
                ))
            }
        }
        other => Err(ConnectorError::ConnectionError(format!(
            "use build_connector_async for {:?}",
            std::mem::discriminant(&other)
        ))),
    }
}

/// Construct a `Box<dyn DatabaseConnector>` from any config, including those
/// that require async connection setup (Postgres, ClickHouse, Snowflake, BigQuery).
pub async fn build_connector_async(
    cfg: ConnectorConfig,
) -> Result<Box<dyn DatabaseConnector>, ConnectorError> {
    match cfg {
        ConnectorConfig::DuckDb(_) => build_connector(cfg),

        #[cfg(feature = "postgres")]
        ConnectorConfig::Postgres(c) => {
            let conn =
                PostgresConnector::new(&c.host, c.port, &c.user, &c.password, &c.database).await?;
            Ok(Box::new(conn))
        }

        #[cfg(feature = "clickhouse")]
        ConnectorConfig::ClickHouse(c) => {
            let conn = ClickHouseConnector::new(c.url, c.user, c.password, c.database).await?;
            Ok(Box::new(conn))
        }

        #[cfg(feature = "snowflake")]
        ConnectorConfig::Snowflake(c) => {
            let conn = SnowflakeConnector::new(
                c.account,
                c.username,
                c.password,
                c.role,
                c.warehouse,
                c.database,
                c.schema,
            )
            .await?;
            Ok(Box::new(conn))
        }

        #[cfg(feature = "bigquery")]
        ConnectorConfig::BigQuery(c) => {
            let conn = BigQueryConnector::new(&c.key_path, c.project_id, c.dataset).await?;
            Ok(Box::new(conn))
        }

        #[allow(unreachable_patterns)]
        other => Err(ConnectorError::ConnectionError(format!(
            "backend not compiled in for {:?}",
            std::mem::discriminant(&other)
        ))),
    }
}
