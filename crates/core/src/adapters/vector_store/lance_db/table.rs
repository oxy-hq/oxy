use std::iter::once;

use super::schema::SchemaUtils;
use crate::config::constants::{RETRIEVAL_INCLUSIONS_TABLE, VECTOR_INDEX_MIN_ROWS};
use crate::errors::OxyError;
use arrow::array::{RecordBatch, RecordBatchIterator};
use lancedb::{
    Connection, Table,
    database::CreateTableMode,
    index::{Index, vector::IvfHnswPqIndexBuilder},
    table::OptimizeAction,
};

pub(super) struct TableManager {
    connection: Connection,
    n_dims: usize,
}

impl TableManager {
    pub(super) fn new(connection: Connection, n_dims: usize) -> Self {
        Self { connection, n_dims }
    }

    pub(super) async fn get_or_create_table(&self, table_name: &str) -> Result<Table, OxyError> {
        let table_result = self
            .connection
            .open_table(table_name.to_string())
            .execute()
            .await;
        let expected_schema = match table_name {
            RETRIEVAL_INCLUSIONS_TABLE => SchemaUtils::create_retrieval_schema(self.n_dims),
            _ => {
                return Err(OxyError::RuntimeError(format!(
                    "Unknown table name for get_or_create_table: {table_name}"
                )));
            }
        };

        let table = match table_result {
            Ok(table) => {
                let existing_schema = table.schema().await?;
                if !SchemaUtils::schemas_match(&expected_schema, &existing_schema) {
                    drop(table);
                    self.connection.drop_table(table_name.to_string()).await?;
                    self.connection
                        .create_empty_table(table_name.to_string(), expected_schema)
                        .mode(CreateTableMode::exist_ok(|builder| builder))
                        .execute()
                        .await?
                } else {
                    table
                }
            }
            Err(err) => match err {
                lancedb::Error::TableNotFound { .. } => {
                    self.connection
                        .create_empty_table(table_name.to_string(), expected_schema)
                        .mode(CreateTableMode::exist_ok(|builder| builder))
                        .execute()
                        .await?
                }
                _ => {
                    return Err(err.into());
                }
            },
        };
        Ok(table)
    }

    pub(super) async fn upsert_batch(
        &self,
        table: &Table,
        batch: RecordBatch,
    ) -> Result<(), OxyError> {
        let schema = batch.schema();
        let reader = RecordBatchIterator::new(once(Ok(batch)), schema);

        let mut merge = table.merge_insert(&["upsert_key"]);
        merge
            .when_matched_update_all(None)
            .when_not_matched_insert_all();
        merge.execute(Box::new(reader)).await?;

        Ok(())
    }

    pub(super) async fn reindex_and_optimize(
        &self,
        table: &Table,
        vector_index_columns: &[&str],
    ) -> Result<(), OxyError> {
        let indices = table.list_indices().await?;
        let num_rows = table.count_rows(None).await?;

        for column in vector_index_columns {
            let has_index = indices
                .iter()
                .any(|index| index.columns.contains(&column.to_string()));
            if !has_index && num_rows >= VECTOR_INDEX_MIN_ROWS {
                table
                    .create_index(
                        &[column.to_string()],
                        Index::IvfHnswPq(IvfHnswPqIndexBuilder::default()),
                    )
                    .execute()
                    .await?;
            }
        }

        let optimization_stats = table.optimize(OptimizeAction::All).await?;
        tracing::info!(
            "Table optimization stats:\n- Compaction: {:?} \n- Prune: {:?}\n",
            &optimization_stats.compaction,
            &optimization_stats.prune
        );

        Ok(())
    }
}
