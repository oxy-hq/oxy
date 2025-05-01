use std::sync::Arc;

use arrow::{array::RecordBatch, datatypes::SchemaRef};
use bigquery::BigQuerySource;
use bigquery_transport::BigQueryArrowTransport;
use connectorx::prelude::{ArrowDestination, CXQuery, Dispatcher, SourceConn, get_arrow};
use uuid::Uuid;

use crate::errors::OxyError;

use super::{
    constants::{
        BIGQUERY_DIALECT, CREATE_CONN, EXECUTE_QUERY, FAILED_TO_RUN_BLOCKING_TASK,
        LOAD_ARROW_RESULT, WRITE_RESULT,
    },
    engine::Engine,
    utils::{connector_internal_error, write_to_ipc},
};

mod bigquery;
mod bigquery_transport;

#[derive(Debug)]
pub(super) struct ConnectorX {
    dialect: String,
    db_path: String,
    dry_run_limit: Option<u64>,
}

impl ConnectorX {
    pub fn new(dialect: String, db_path: String, dry_run_limit: Option<u64>) -> Self {
        Self {
            dialect,
            db_path,
            dry_run_limit,
        }
    }
}

impl Engine for ConnectorX {
    async fn dry_run(&self, query: &str) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        match self.dialect.as_str() {
            BIGQUERY_DIALECT => {
                let conn_string = format!("{}://{}", self.dialect, self.db_path);
                let rt =
                    Arc::new(tokio::runtime::Runtime::new().map_err(|err| {
                        connector_internal_error(FAILED_TO_RUN_BLOCKING_TASK, &err)
                    })?);
                let query = query.to_string();

                tokio::task::spawn_blocking(move || {
                    let mut source = BigQuerySource::new(rt, &conn_string, None)
                        .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
                    source
                        .dry_run(&query)
                        .map_err(|err| connector_internal_error(EXECUTE_QUERY, &err))
                })
                .await
                .map_err(|err| connector_internal_error(FAILED_TO_RUN_BLOCKING_TASK, &err))?
            }
            _ => self.explain_query(query).await,
        }
    }

    async fn explain_query(&self, query: &str) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        match self.dialect.as_str() {
            BIGQUERY_DIALECT => Box::pin(self.dry_run(query)).await, // We're forming the loop here because the dialect is bigquery
            _ => {
                let explain_query = format!("EXPLAIN ({})", query.trim().trim_end_matches(';'));
                self.run_query_with_limit(&explain_query, None).await
            }
        }
    }

    async fn run_query(&self, query: &str) -> Result<String, OxyError> {
        let (record_batches, schema_ref) =
            self.run_query_with_limit(query, self.dry_run_limit).await?;
        let file_path = format!("/tmp/{}.arrow", Uuid::new_v4());
        write_to_ipc(&record_batches, &file_path, &schema_ref)
            .map_err(|err| connector_internal_error(WRITE_RESULT, err))?;
        Ok(file_path)
    }

    async fn run_query_with_limit(
        &self,
        query: &str,
        dry_run_limit: Option<u64>,
    ) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        let conn_string = format!("{}://{}", self.dialect, self.db_path);
        let query = query.to_string();
        let dialect = self.dialect.clone();
        let result = tokio::task::spawn_blocking(move || {
            let destination = match dialect.as_str() {
                BIGQUERY_DIALECT => {
                    let mut destination = ArrowDestination::new();
                    let rt = Arc::new(tokio::runtime::Runtime::new().map_err(|err| {
                        connector_internal_error(FAILED_TO_RUN_BLOCKING_TASK, err)
                    })?);
                    let source = BigQuerySource::new(rt, &conn_string, dry_run_limit)
                        .map_err(|err| connector_internal_error(CREATE_CONN, err))?;
                    let queries = &[query.as_str()];
                    let dispatcher = Dispatcher::<_, _, BigQueryArrowTransport>::new(
                        source,
                        &mut destination,
                        queries,
                        None,
                    );
                    dispatcher
                        .run()
                        .map_err(|err| connector_internal_error(EXECUTE_QUERY, err))?;
                    Ok(destination)
                }
                _ => {
                    let source_conn = SourceConn::try_from(conn_string.as_str())
                        .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
                    let queries = &[CXQuery::from(query.as_str())];
                    get_arrow(&source_conn, None, queries, None)
                        .map_err(|err| connector_internal_error(EXECUTE_QUERY, &err))
                }
            }?;
            let schema = destination.arrow_schema();
            let result = destination
                .arrow()
                .map_err(|err| connector_internal_error(LOAD_ARROW_RESULT, &err))?;

            Result::<_, OxyError>::Ok((result, schema))
        })
        .await
        .map_err(|e| connector_internal_error(FAILED_TO_RUN_BLOCKING_TASK, &e))??;

        Ok(result)
    }
}
