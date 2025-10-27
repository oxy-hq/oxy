use arrow::array::RecordBatch;
use arrow::datatypes::SchemaRef;
use duckdb::Connection;

use crate::adapters::connector::constants::{CREATE_CONN, EXECUTE_QUERY, PREPARE_DUCKDB_STMT};
use crate::adapters::connector::utils::connector_internal_error;
use crate::adapters::secrets::SecretsManager;
use crate::config::model::MotherDuck as MotherDuckConfig;
use crate::errors::OxyError;

use super::engine::Engine;

#[derive(Debug)]
pub(super) struct MotherDuck {
    token: String,
    database: Option<String>,
}

impl MotherDuck {
    pub async fn from_config(
        secrets_manager: SecretsManager,
        config: MotherDuckConfig,
    ) -> Result<Self, OxyError> {
        let token = config.get_token(&secrets_manager).await?;
        Ok(Self {
            token,
            database: config.database,
        })
    }

    fn get_connection_string(&self) -> String {
        let base = match &self.database {
            Some(db) => format!("md:{}", db),
            None => "md:".to_string(),
        };
        format!("{}?motherduck_token={}", base, self.token)
    }
}

impl Engine for MotherDuck {
    async fn run_query_with_limit(
        &self,
        query: &str,
        _dry_run_limit: Option<u64>,
    ) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        let query = query.to_string();
        let connection_string = self.get_connection_string();

        // Run blocking database operations in a spawned thread
        tokio::task::spawn_blocking(move || {
            let conn = Connection::open(connection_string)
                .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;

            let mut stmt = conn
                .prepare(&query)
                .map_err(|err| connector_internal_error(PREPARE_DUCKDB_STMT, &err))?;

            let arrow_stream = stmt
                .query_arrow([])
                .map_err(|err| connector_internal_error(EXECUTE_QUERY, &err))?;

            let schema = arrow_stream.get_schema();
            let arrow_chunks = arrow_stream.collect();

            tracing::debug!("MotherDuck query results: {:?}", arrow_chunks);
            Ok((arrow_chunks, schema))
        })
        .await
        .map_err(|err| OxyError::RuntimeError(format!("Task join error: {}", err)))?
    }
}
