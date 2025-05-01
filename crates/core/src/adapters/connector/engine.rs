use arrow::{array::RecordBatch, datatypes::SchemaRef};
use uuid::Uuid;

use crate::errors::OxyError;

use super::{
    constants::WRITE_RESULT,
    utils::{connector_internal_error, write_to_ipc},
};

#[enum_dispatch::enum_dispatch]
pub(super) trait Engine {
    async fn run_query(&self, query: &str) -> Result<String, OxyError> {
        let (record_batches, schema_ref) = self.run_query_with_limit(query, None).await?;
        let file_path = format!("/tmp/{}.arrow", Uuid::new_v4());
        write_to_ipc(&record_batches, &file_path, &schema_ref)
            .map_err(|err| connector_internal_error(WRITE_RESULT, &err))?;
        Ok(file_path)
    }
    async fn run_query_with_limit(
        &self,
        query: &str,
        dry_run_limit: Option<u64>,
    ) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError>;
    async fn run_query_and_load(
        &self,
        query: &str,
    ) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        self.run_query_with_limit(query, None).await
    }
    async fn explain_query(&self, query: &str) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        let explain_query = format!("EXPLAIN ({})", query.trim().trim_end_matches(';'));
        self.run_query_with_limit(&explain_query, None).await
    }
    async fn dry_run(&self, query: &str) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        self.explain_query(query).await
    }
}
