use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use clickhouse::ClickHouse;
use connectorx::ConnectorX;
pub use domo::DOMO;
use duckdb::DuckDB;
use engine::Engine;
use motherduck::MotherDuck;
use snowflake::Snowflake;
use std::collections::HashMap;

use crate::adapters::secrets::SecretsManager;
use crate::adapters::session_filters::{FilterProcessor, SessionFilters};
use crate::config::ConfigManager;
use crate::config::model::DatabaseType;
use crate::errors::OxyError;

mod clickhouse;
mod connectorx;
mod constants;
mod domo;
mod duckdb;
mod engine;
mod motherduck;
mod snowflake;
mod utils;

pub use utils::{load_result, write_to_ipc};

#[enum_dispatch::enum_dispatch(Engine)]
#[derive(Debug)]
enum EngineType {
    DuckDB,
    ConnectorX,
    ClickHouse,
    Snowflake,
    DOMO,
    MotherDuck,
}

#[derive(Debug)]
pub struct Connector {
    engine: EngineType,
}

impl Connector {
    pub async fn from_database(
        database_ref: &str,
        config_manager: &ConfigManager,
        secrets_manager: &SecretsManager,
        dry_run_limit: Option<u64>,
        filters: Option<SessionFilters>,
    ) -> Result<Self, OxyError> {
        let database = config_manager.resolve_database(database_ref)?;
        let engine = match &database.database_type {
            DatabaseType::Bigquery(bigquery) => {
                let key_path_str = bigquery.get_key_path(secrets_manager).await?;
                let key_path = if bigquery.key_path.is_some() {
                    config_manager.resolve_file(&key_path_str).await?
                } else {
                    key_path_str
                };
                println!("BigQuery key path resolved: {}", key_path);
                EngineType::ConnectorX(ConnectorX::new(
                    database.dialect(),
                    key_path,
                    dry_run_limit.or(bigquery.dry_run_limit),
                ))
            }
            DatabaseType::DuckDB(duckdb) => EngineType::DuckDB(DuckDB::new(
                config_manager
                    .resolve_file(&duckdb.file_search_path)
                    .await?,
            )),
            DatabaseType::Postgres(pg) => {
                let db_path = format!(
                    "{}:{}@{}:{}/{}",
                    pg.get_user(secrets_manager).await?,
                    pg.get_password(secrets_manager).await?,
                    pg.get_host(secrets_manager).await?,
                    pg.get_port(secrets_manager).await?,
                    pg.get_database(secrets_manager).await?,
                );
                EngineType::ConnectorX(ConnectorX::new(database.dialect(), db_path, None))
            }
            DatabaseType::Redshift(rs) => {
                let db_path = format!(
                    "{}:{}@{}:{}/{}?cxprotocol={}",
                    rs.get_user(secrets_manager).await?,
                    rs.get_password(secrets_manager).await?,
                    rs.get_host(secrets_manager).await?,
                    rs.get_port(secrets_manager).await?,
                    rs.get_database(secrets_manager).await?,
                    // https://github.com/sfu-db/connector-x/blob/534617477f78b092ba169c71e64778b86d5853ad/connectorx-python/connectorx/__init__.py#L50-L66
                    // redshift only supports cursor protocol
                    "cursor"
                );
                EngineType::ConnectorX(ConnectorX::new(database.dialect(), db_path, None))
            }
            DatabaseType::Mysql(my) => {
                let db_path = format!(
                    "{}:{}@{}:{}/{}",
                    my.get_user(secrets_manager).await?,
                    my.get_password(secrets_manager).await?,
                    my.get_host(secrets_manager).await?,
                    my.get_port(secrets_manager).await?,
                    my.get_database(secrets_manager).await?,
                );
                EngineType::ConnectorX(ConnectorX::new(database.dialect(), db_path, None))
            }
            DatabaseType::ClickHouse(ch) => {
                let validated = Self::validate_filters(database_ref, &ch.filters, filters)?;
                let mut clickhouse = ClickHouse::new(ch.clone(), secrets_manager.clone());
                if let Some(filters) = validated {
                    clickhouse = clickhouse.with_filters(filters);
                }
                EngineType::ClickHouse(clickhouse)
            }
            DatabaseType::Snowflake(snowflake) => EngineType::Snowflake(Snowflake::new(
                snowflake.clone(),
                secrets_manager.clone(),
                config_manager.clone(),
            )),
            DatabaseType::DOMO(domo) => {
                EngineType::DOMO(DOMO::from_config(secrets_manager.clone(), domo.clone()).await?)
            }
            DatabaseType::MotherDuck(motherduck) => EngineType::MotherDuck(
                MotherDuck::from_config(secrets_manager.clone(), motherduck.clone()).await?,
            ),
        };
        Ok(Connector { engine })
    }

    pub async fn run_query(&self, query: &str) -> Result<String, OxyError> {
        self.engine.run_query(query).await
    }

    pub async fn run_query_with_limit(
        &self,
        query: &str,
        dry_run_limit: Option<u64>,
    ) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        self.engine.run_query_with_limit(query, dry_run_limit).await
    }

    pub async fn run_query_and_load(
        &self,
        query: &str,
    ) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        self.engine.run_query_and_load(query).await
    }

    pub async fn explain_query(
        &self,
        query: &str,
    ) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        self.engine.explain_query(query).await
    }

    pub async fn dry_run(&self, query: &str) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        self.engine.dry_run(query).await
    }

    /// Validate api request filters against configured database filter schemas
    fn validate_filters(
        database_ref: &str,
        schemas: &HashMap<String, schemars::schema::SchemaObject>,
        filters: Option<SessionFilters>,
    ) -> Result<Option<SessionFilters>, OxyError> {
        let Some(filters) = filters else {
            // Log when no filters provided (may be required for some databases)
            if !schemas.is_empty() {
                tracing::debug!(
                    database = %database_ref,
                    configured_filters = ?schemas.keys().collect::<Vec<_>>(),
                    "No filters provided for database with filter schema"
                );
            }
            return Ok(None);
        };

        if schemas.is_empty() {
            // Security event: filters provided but not configured
            tracing::warn!(
                database = %database_ref,
                provided_filters = ?filters.keys().collect::<Vec<_>>(),
                "Filters provided for database but no filter schema configured - ignoring filters"
            );
            return Ok(None);
        }

        // Log filter validation attempt for audit trail
        tracing::info!(
            database = %database_ref,
            provided_filters = ?filters.keys().collect::<Vec<_>>(),
            configured_filters = ?schemas.keys().collect::<Vec<_>>(),
            "Validating filters for database query"
        );

        let processor = FilterProcessor::new(schemas.clone());
        let validated = processor.process_filters(filters).map_err(|e| {
            // Log filter validation failure as security event
            tracing::error!(
                database = %database_ref,
                error = %e,
                "Filter validation failed - rejecting request"
            );
            e
        })?;

        // Log successful filter validation for audit trail
        tracing::info!(
            database = %database_ref,
            validated_filters = ?validated.keys().collect::<Vec<_>>(),
            "Filter validation successful - applying filters to query"
        );

        Ok(Some(validated))
    }
}
