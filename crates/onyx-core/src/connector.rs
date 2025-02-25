use arrow::datatypes::{Schema, SchemaRef};
use arrow::ipc::{reader::FileReader, writer::FileWriter};
use arrow::{array::as_string_array, error::ArrowError, record_batch::RecordBatch};
use connectorx::prelude::{get_arrow, CXQuery, SourceConn};
use duckdb::Connection;
use log::debug;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

use crate::config::model::{Config, Database, DatabaseType};

const GET_CURRENT_DIR: &str = "Failed to get current directory";
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

fn connector_internal_error(message: &str, e: &impl std::fmt::Display) -> anyhow::Error {
    log::error!("{}: {}", message, e);
    anyhow::Error::msg(format!("{}: {}", message, e))
}

pub struct Connector {
    database_config: Database,
    config: Config,
}

#[derive(serde::Serialize, Clone)]
pub struct DatabaseInfo {
    name: String,
    dialect: String,
    tables: Vec<String>,
}

impl Connector {
    pub fn new(database_config: &Database, config: &Config) -> Self {
        Connector {
            database_config: database_config.clone(),
            config: config.clone(),
        }
    }

    pub async fn load_database_info(&self) -> DatabaseInfo {
        let tables = self.get_schemas().await;
        let name = self.database_config.dataset.clone();
        let dialect = self.database_config.database_type.to_string();
        DatabaseInfo {
            name,
            dialect,
            tables,
        }
    }

    pub async fn list_datasets(&self) -> Vec<String> {
        let query_string = match self.database_config.database_type {
            DatabaseType::Bigquery(_) => {
                "SELECT schema_name FROM INFORMATION_SCHEMA.SCHEMATA".to_owned()
            }
            DatabaseType::DuckDB(_) => "".to_owned(),
        };
        self.run_query_and_collect(query_string)
            .await
            .unwrap_or_default()
    }

    pub async fn get_schemas(&self) -> Vec<String> {
        let query_string = match self.database_config.database_type {
            DatabaseType::Bigquery(_) => format!(
                "SELECT ddl FROM `{}`.INFORMATION_SCHEMA.TABLES",
                self.database_config.dataset
            ),
            DatabaseType::DuckDB(_) => "".to_owned(),
            _ => "".to_owned(),
        };
        self.run_query_and_collect(query_string)
            .await
            .unwrap_or_default()
    }

    async fn run_query_and_collect(&self, query_string: String) -> Option<Vec<String>> {
        if query_string.is_empty() {
            return None;
        }
        let (datasets, _) = self.run_query_and_load(&query_string).await.ok()?;
        let result_iter = datasets
            .iter()
            .flat_map(|batch| as_string_array(batch.column(0)).iter());
        Some(
            result_iter
                .map(|name| name.map(|s| s.to_string()))
                .collect::<Option<Vec<String>>>()
                .unwrap_or_default(),
        )
    }

    pub async fn run_query(&self, query: &str) -> anyhow::Result<String> {
        match &self.database_config.database_type {
            DatabaseType::Bigquery(bigquery) => {
                let key_path = self.config.project_path.join(&bigquery.key_path);
                self.run_connectorx_query(query, key_path).await
            }
            DatabaseType::DuckDB(_) => self.run_duckdb_query(query).await,
        }
    }

    pub async fn run_query_and_load(
        &self,
        query: &str,
    ) -> anyhow::Result<(Vec<RecordBatch>, SchemaRef)> {
        let file_path = self.run_query(query).await?;
        load_result(&file_path).map_err(|e| connector_internal_error(LOAD_RESULT, &e))
    }

    async fn run_connectorx_query(&self, query: &str, key_path: PathBuf) -> anyhow::Result<String> {
        let current_dir = std::env::current_dir()
            .map_err(|err| connector_internal_error(GET_CURRENT_DIR, &err))?;
        let key_path = current_dir.join(&key_path);
        let conn_string = format!(
            "{}://{}",
            self.database_config.database_type,
            key_path.to_str().unwrap()
        );
        self.run_query_with_connectorx(conn_string, query.to_string())
            .await
    }

    async fn run_query_with_connectorx(
        &self,
        conn_string: String,
        query: String,
    ) -> anyhow::Result<String> {
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

    async fn run_duckdb_query(&self, query: &str) -> anyhow::Result<String> {
        let query = query.to_string();
        let conn = Connection::open_in_memory()
            .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
        let dir_set_stmt = format!(
            "SET file_search_path = '{}'",
            self.config
                .project_path
                .join(&self.database_config.dataset)
                .display()
        );
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
