use arrow::array::RecordBatch;
use arrow::datatypes::SchemaRef;
use duckdb::Connection;

use crate::adapters::connector::constants::{
    CREATE_CONN, EXECUTE_QUERY, PREPARE_DUCKDB_STMT, SET_FILE_SEARCH_PATH,
};
use crate::adapters::connector::utils::connector_internal_error;
use crate::errors::OxyError;

use super::engine::Engine;

#[derive(Debug)]
pub(super) struct DuckDB {
    file_search_path: String,
}

impl DuckDB {
    pub fn new(file_search_path: String) -> Self {
        DuckDB { file_search_path }
    }
}

impl Engine for DuckDB {
    async fn run_query_with_limit(
        &self,
        query: &str,
        _dry_run_limit: Option<u64>,
    ) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
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
        tracing::debug!("Query results: {:?}", arrow_chunks);
        Ok((arrow_chunks, schema))
    }
}
