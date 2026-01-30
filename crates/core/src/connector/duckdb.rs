use arrow::array::RecordBatch;
use arrow::datatypes::SchemaRef;
use duckdb::Connection;
use slugify::slugify;

use super::engine::Engine;
use crate::adapters::secrets::SecretsManager;
use crate::config::model::DuckDBOptions;
use crate::connector::constants::{
    CREATE_CONN, CREATE_TEMP_TABLE, EXECUTE_QUERY, PREPARE_DUCKDB_STMT, SET_FILE_SEARCH_PATH,
};
use crate::connector::utils::connector_internal_error;
use oxy_shared::errors::OxyError;

#[derive(Debug)]
pub(super) struct DuckDB {
    options: DuckDBOptions,
    secrets_manager: SecretsManager,
}

impl DuckDB {
    pub fn new(options: DuckDBOptions, secrets_manager: SecretsManager) -> Self {
        DuckDB {
            options,
            secrets_manager,
        }
    }

    fn create_temp_table_from_file(
        &self,
        conn: &Connection,
        file_search_path: &str,
        file_path: &str,
    ) -> Result<(), OxyError> {
        // Extract file name to use as table name by:
        // Example: file_search_path = "data/", file_path = "data//sales/january.csv" -> table_name = "sales__january.csv"
        let normalized_file_path = std::path::Path::new(file_path)
            .components()
            .collect::<std::path::PathBuf>();
        let relative_path = normalized_file_path
            .strip_prefix(file_search_path)
            .map_err(|err| connector_internal_error(CREATE_TEMP_TABLE, &err))?
            .to_string_lossy()
            .to_string();

        let create_stmt = format!(
            "CREATE TEMPORARY TABLE '{}' AS FROM '{}'",
            slugify!(&relative_path, separator = "_"),
            file_path
        );
        tracing::info!(
            "Creating temporary table from file: {} with statement: {}",
            file_path,
            create_stmt
        );
        conn.execute(&create_stmt, [])
            .map_err(|err| connector_internal_error(CREATE_TEMP_TABLE, &err))?;
        Ok(())
    }

    pub async fn init_connection(&self) -> Result<Connection, OxyError> {
        let conn = match &self.options {
            DuckDBOptions::Local { file_search_path } => {
                let conn = Connection::open_in_memory()
                    .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
                let dir_set_stmt = format!("SET file_search_path = '{}';", &file_search_path);
                conn.execute(&dir_set_stmt, [])
                    .map_err(|err| connector_internal_error(SET_FILE_SEARCH_PATH, &err))?;
                let temp_set_stmt = format!("SET temp_directory = '{}/tmp';", &file_search_path);
                conn.execute(&temp_set_stmt, [])
                    .map_err(|err| connector_internal_error(SET_FILE_SEARCH_PATH, &err))?;
                let file_paths = {
                    let mut stmt = conn
                        .prepare("SELECT * FROM glob('*')")
                        .map_err(|e| connector_internal_error(CREATE_CONN, &e))?;
                    let rows = stmt
                        .query_map([], |row| row.get(0))
                        .map_err(|e| connector_internal_error(CREATE_CONN, &e))?;

                    let mut file_paths: Vec<String> = vec![];
                    for row in rows {
                        let file_path: String =
                            row.map_err(|e| connector_internal_error(CREATE_CONN, &e))?;
                        file_paths.push(file_path);
                    }
                    file_paths
                };

                for file_path in file_paths {
                    self.create_temp_table_from_file(&conn, file_search_path, &file_path)?;
                }
                tracing::debug!(
                    "Initialized DuckDB with file search path '{}'",
                    file_search_path,
                );
                conn
            }
            DuckDBOptions::DuckLake(config) => {
                let conn = Connection::open_in_memory()
                    .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
                conn.execute("INSTALL ducklake", [])
                    .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
                conn.execute("LOAD ducklake", [])
                    .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
                conn.execute("INSTALL postgres", [])
                    .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
                conn.execute("LOAD postgres", [])
                    .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
                // Retrieve secrets and generate attach statements
                let attach_stmt = config.to_duckdb_attach_stmt(&self.secrets_manager).await?;
                tracing::info!("Executing DuckDB attach statement: {:?}", attach_stmt);
                for stmt in attach_stmt {
                    tracing::debug!("Executing DuckDB statement: {}", stmt);
                    conn.execute(&stmt, [])
                        .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
                }
                conn
            }
        };
        conn.execute("INSTALL icu", [])
            .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
        conn.execute("LOAD icu", [])
            .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
        Ok(conn)
    }
}

impl Engine for DuckDB {
    async fn run_query_with_limit(
        &self,
        query: &str,
        _dry_run_limit: Option<u64>,
    ) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        let query = query.to_string();

        let conn = self.init_connection().await?;
        let mut stmt = conn
            .prepare(&query)
            .map_err(|err| connector_internal_error(PREPARE_DUCKDB_STMT, &err))?;
        let arrow_stream = stmt
            .query_arrow([])
            .map_err(|err| connector_internal_error(EXECUTE_QUERY, &err))?;
        let schema = arrow_stream.get_schema();
        let arrow_chunks = arrow_stream.collect();
        tracing::debug!("Query results: {:?}", arrow_chunks);
        Ok((arrow_chunks, schema))
    }
}
