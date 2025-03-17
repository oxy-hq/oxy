use arrow::datatypes::{Schema, SchemaRef};
use arrow::ipc::{reader::FileReader, writer::FileWriter};
use arrow::{array::as_string_array, error::ArrowError, record_batch::RecordBatch};
use connectorx::prelude::{get_arrow, CXQuery, SourceConn};
use duckdb::Connection;
use log::debug;
use std::fs::File;
use std::sync::Arc;
use uuid::Uuid;

use crate::config::model::{Database, DatabaseType};
use crate::config::ConfigManager;
use crate::errors::OxyError;

const CREATE_CONN: &str = "Failed to open connection";
const EXECUTE_QUERY: &str = "Failed to execute query";
const LOAD_RESULT: &str = "Error loading query results";
const WRITE_RESULT: &str = "Failed to write result to IPC";
const SET_FILE_SEARCH_PATH: &str = "Failed to set file search path";
const FAILED_TO_RUN_BLOCKING_TASK: &str = "Failed to run blocking task";

// duckdb errors
const PREPARE_DUCKDB_STMT: &str = "Failed to prepare DuckDB statement";

// arrow errors
const LOAD_ARROW_RESULT: &str = "Failed to load arrow result";

fn connector_internal_error(message: &str, e: &impl std::fmt::Display) -> OxyError {
    log::error!("{}: {}", message, e);
    OxyError::DBError(format!("{}: {}", message, e))
}

#[enum_dispatch::enum_dispatch]
trait Engine {
    async fn run_query(&self, query: &str) -> Result<String, OxyError>;
    async fn load_database_info(&self) -> Result<DatabaseInfo, OxyError>;
    async fn run_query_and_load(
        &self,
        query: &str,
    ) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        let file_path = self.run_query(query).await?;
        load_result(&file_path).map_err(|e| connector_internal_error(LOAD_RESULT, &e))
    }
}

#[enum_dispatch::enum_dispatch(Engine)]
#[derive(Debug)]
enum EngineType {
    DuckDB,
    ConnectorX,
}

#[derive(Debug)]
struct DuckDB {
    file_search_path: String,
}

impl Engine for DuckDB {
    async fn run_query(&self, query: &str) -> Result<String, OxyError> {
        let query = query.to_string();
        let conn = Connection::open_in_memory()
            .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
        let dir_set_stmt = format!("SET file_search_path = '{}'", &self.file_search_path);
        conn.execute(&dir_set_stmt, [])
            .map_err(|err| connector_internal_error(SET_FILE_SEARCH_PATH, &err))?;
        let mut stmt = conn
            .prepare(&query)
            .map_err(|err| connector_internal_error(PREPARE_DUCKDB_STMT, &err))?;
        let arrow_stream = stmt
            .query_arrow([])
            .map_err(|err| connector_internal_error(EXECUTE_QUERY, &err))?;
        let schema = arrow_stream.get_schema();
        let arrow_chunks = arrow_stream.collect();
        debug!("Query results: {:?}", arrow_chunks);
        let file_path = format!("/tmp/{}.arrow", Uuid::new_v4());
        write_to_ipc(&arrow_chunks, &file_path, &schema)
            .map_err(|err| connector_internal_error(WRITE_RESULT, &err))?;
        Ok(file_path)
    }

    async fn load_database_info(&self) -> Result<DatabaseInfo, OxyError> {
        Ok(DatabaseInfo {
            name: self.file_search_path.to_string(),
            dialect: "duckdb".to_string(),
            tables: vec![],
        })
    }
}

#[derive(Debug)]
pub struct ConnectorX {
    dialect: String,
    db_path: String,
    db_name: String,
}

impl ConnectorX {
    pub async fn get_schemas(&self) -> Result<Vec<String>, OxyError> {
        let query_string = match self.dialect.as_str() {
            "bigquery" => {
                format!(
                    "SELECT ddl FROM `{}`.INFORMATION_SCHEMA.TABLES",
                    self.db_name
                )
            }
            "postgres" => {
                "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'"
                    .to_string()
            }
            _ => Err(OxyError::DBError(format!(
                "Unsupported dialect: {}",
                self.dialect
            )))?,
        };
        let (datasets, _) = self.run_query_and_load(&query_string).await?;
        let result_iter = datasets
            .iter()
            .flat_map(|batch| as_string_array(batch.column(0)).iter());
        Ok(result_iter
            .map(|name| name.map(|s| s.to_string()))
            .collect::<Option<Vec<String>>>()
            .unwrap_or_default())
    }
}

impl Engine for ConnectorX {
    async fn run_query(&self, query: &str) -> Result<String, OxyError> {
        let conn_string = format!("{}://{}", self.dialect, self.db_path);
        let query = query.to_string();
        let result = tokio::task::spawn_blocking(move || {
            let source_conn = SourceConn::try_from(conn_string.as_str())
                .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
            let queries = &[CXQuery::from(query.as_str())];
            let destination = get_arrow(&source_conn, None, queries, None)
                .map_err(|err| connector_internal_error(EXECUTE_QUERY, &err))?;
            let schema = destination.arrow_schema();
            let file_path = format!("/tmp/{}.arrow", Uuid::new_v4());
            let result = destination
                .arrow()
                .map_err(|err| connector_internal_error(LOAD_ARROW_RESULT, &err))?;

            write_to_ipc(&result, &file_path, &schema)
                .map_err(|err| connector_internal_error(WRITE_RESULT, &err))?;
            Ok::<String, anyhow::Error>(file_path)
        })
        .await
        .map_err(|e| connector_internal_error(FAILED_TO_RUN_BLOCKING_TASK, &e))??;

        Ok(result)
    }

    async fn load_database_info(&self) -> Result<DatabaseInfo, OxyError> {
        Ok(DatabaseInfo {
            name: self.db_name.to_string(),
            dialect: self.dialect.to_string(),
            tables: self.get_schemas().await?,
        })
    }
}

#[derive(Debug)]
pub struct Connector {
    engine: EngineType,
}

#[derive(serde::Serialize, Clone)]
pub struct DatabaseInfo {
    name: String,
    dialect: String,
    tables: Vec<String>,
}

impl Connector {
    pub async fn from_database(
        database_ref: &str,
        config_manager: &ConfigManager,
    ) -> Result<Self, OxyError> {
        let database = config_manager.resolve_database(database_ref)?;
        let engine = match &database.database_type {
            DatabaseType::Bigquery(bigquery) => {
                let key_path = config_manager
                    .resolve_file(
                        bigquery
                            .key_path
                            .as_ref()
                            .ok_or(OxyError::DBError("Key path not set".to_string()))?,
                    )
                    .await?;
                EngineType::ConnectorX(ConnectorX {
                    dialect: database.dialect(),
                    db_path: key_path,
                    db_name: bigquery.dataset.clone(),
                })
            }
            DatabaseType::DuckDB(duckdb) => EngineType::DuckDB(DuckDB {
                file_search_path: config_manager
                    .resolve_file(&duckdb.file_search_path)
                    .await?,
            }),
            DatabaseType::Postgres(_pg) => {
                let conn_string = Database::postgres_family_conn_string(database);
                EngineType::ConnectorX(ConnectorX {
                    dialect: database.dialect(),
                    db_path: conn_string,
                    db_name: Database::postgres_family_db_name(database),
                })
            }
            DatabaseType::Redshift(_rs) => {
                let conn_string = Database::postgres_family_conn_string(database);
                EngineType::ConnectorX(ConnectorX {
                    dialect: database.dialect(),
                    db_path: conn_string,
                    db_name: Database::postgres_family_db_name(database),
                })
            }
        };
        Ok(Connector { engine })
    }

    pub async fn database_info(&self) -> Result<DatabaseInfo, OxyError> {
        self.engine.load_database_info().await
    }

    pub async fn run_query(&self, query: &str) -> Result<String, OxyError> {
        self.engine.run_query(query).await
    }

    pub async fn run_query_and_load(
        &self,
        query: &str,
    ) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        self.engine.run_query_and_load(query).await
    }
}

pub fn load_result(file_path: &str) -> anyhow::Result<(Vec<RecordBatch>, SchemaRef)> {
    let file = File::open(file_path).map_err(|_| {
        anyhow::Error::msg("Executed query did not generate a valid output file. If you are using an agent to generate the query, consider giving it a shorter prompt.".to_string())
    })?;
    let reader = FileReader::try_new(file, None)?;
    let schema = reader.schema();
    // Collect results and handle potential errors
    let batches: Result<Vec<RecordBatch>, ArrowError> = reader.collect();
    let batches = batches?;

    Ok((batches, schema))
}

fn write_to_ipc(
    batches: &Vec<RecordBatch>,
    file_path: &str,
    schema: &Arc<Schema>,
) -> anyhow::Result<()> {
    let file = File::create(file_path)?;
    if batches.is_empty() {
        debug!("Warning: query returned no results.");
    }

    debug!("Schema: {:?}", schema);
    let schema_ref = schema.as_ref();
    let mut writer = FileWriter::try_new(file, schema_ref)?;
    for batch in batches {
        writer.write(batch)?;
    }
    writer.finish()?;
    Ok(())
}
