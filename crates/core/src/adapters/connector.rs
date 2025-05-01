use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use clickhouse::ClickHouse;
use connectorx::ConnectorX;
use duckdb::DuckDB;
use engine::Engine;
use snowflake::Snowflake;

use crate::config::ConfigManager;
use crate::config::model::DatabaseType;
use crate::errors::OxyError;

mod clickhouse;
mod connectorx;
mod constants;
mod duckdb;
mod engine;
mod snowflake;
mod utils;

pub use utils::load_result;

#[enum_dispatch::enum_dispatch(Engine)]
#[derive(Debug)]
enum EngineType {
    DuckDB,
    ConnectorX,
    ClickHouse,
    Snowflake,
}

#[derive(Debug)]
pub struct Connector {
    engine: EngineType,
}

impl Connector {
    pub async fn from_database(
        database_ref: &str,
        config_manager: &ConfigManager,
        dry_run_limit: Option<u64>,
    ) -> Result<Self, OxyError> {
        let database = config_manager.resolve_database(database_ref)?;
        let engine = match &database.database_type {
            DatabaseType::Bigquery(bigquery) => {
                let key_path = config_manager.resolve_file(&bigquery.key_path).await?;
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
                let db_name = pg.database.clone().unwrap_or_default();
                let db_path = format!(
                    "{}:{}@{}:{}/{}",
                    pg.user.clone().unwrap_or_default(),
                    pg.get_password().unwrap_or_default(),
                    pg.host.clone().unwrap_or_default(),
                    pg.port.clone().unwrap_or_default(),
                    db_name,
                );
                EngineType::ConnectorX(ConnectorX::new(database.dialect(), db_path, None))
            }
            DatabaseType::Redshift(rs) => {
                let db_name = rs.database.clone().unwrap_or_default();
                let db_path = format!(
                    "{}:{}@{}:{}/{}?cxprotocol={}",
                    rs.user.clone().unwrap_or_default(),
                    rs.get_password().unwrap_or_default(),
                    rs.host.clone().unwrap_or_default(),
                    rs.port.clone().unwrap_or_default(),
                    db_name,
                    // https://github.com/sfu-db/connector-x/blob/534617477f78b092ba169c71e64778b86d5853ad/connectorx-python/connectorx/__init__.py#L50-L66
                    // redshift only supports cursor protocol
                    "cursor"
                );
                EngineType::ConnectorX(ConnectorX::new(database.dialect(), db_path, None))
            }
            DatabaseType::Mysql(my) => {
                let db_name = my.database.clone().unwrap_or_default();
                let db_path = format!(
                    "{}:{}@{}:{}/{}",
                    my.user.clone().unwrap_or_default(),
                    my.get_password().unwrap_or_default(),
                    my.host.clone().unwrap_or_default(),
                    my.port.clone().unwrap_or_default(),
                    db_name
                );
                EngineType::ConnectorX(ConnectorX::new(database.dialect(), db_path, None))
            }
            DatabaseType::ClickHouse(ch) => EngineType::ClickHouse(ClickHouse::new(ch.clone())),
            DatabaseType::Snowflake(snowflake) => {
                EngineType::Snowflake(Snowflake::new(snowflake.clone()))
            }
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
}
