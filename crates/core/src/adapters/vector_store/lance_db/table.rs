use super::schema::SchemaUtils;
use crate::config::constants::VECTOR_INDEX_MIN_ROWS;
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
    table_name: String,
    n_dims: usize,
}

impl TableManager {
    pub(super) fn new(connection: Connection, table_name: String, n_dims: usize) -> Self {
        Self { connection, table_name, n_dims }
    }

    pub(super) async fn get_or_create_retrieval_table(&self) -> Result<Table, OxyError> {
        let table_result = self.connection.open_table(self.table_name.clone()).execute().await;
        let expected_schema = SchemaUtils::create_retrieval_schema(self.n_dims);

        let table = match table_result {
            Ok(table) => {
                let existing_schema = table.schema().await?;

                if !SchemaUtils::schemas_match(&expected_schema, &existing_schema) {
                    drop(table);
                    self.connection.drop_table(self.table_name.clone()).await?;

                    self.connection
                        .create_empty_table(self.table_name.clone(), expected_schema)
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
                        .create_empty_table(self.table_name.clone(), expected_schema)
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

    pub(super) async fn replace_with_batch(
        &self,
        batch: Option<RecordBatch>,
        vector_column: &str,
    ) -> Result<(), OxyError> {
        let retrieval_table = self.get_or_create_retrieval_table().await?;

        retrieval_table.delete("true").await?;

        if let Some(batch) = batch {
            let schema = batch.schema();
            let reader = RecordBatchIterator::new(
                std::iter::once(Ok(batch)),
                schema,
            );
            retrieval_table.add(Box::new(reader)).execute().await?;

            self.ensure_vector_index_and_optimize(&retrieval_table, vector_column).await?;
        }

        Ok(())
    }

    async fn ensure_vector_index_and_optimize(
        &self,
        table: &Table,
        vector_column: &str,
    ) -> Result<(), OxyError> {
        let indices = table.list_indices().await?;
        let num_rows = table.count_rows(None).await?;

        let has_vector_index = indices
            .iter()
            .any(|index| index.columns.contains(&vector_column.to_string()));

        if !has_vector_index && num_rows >= VECTOR_INDEX_MIN_ROWS {
            table
                .create_index(
                    &[vector_column],
                    Index::IvfHnswPq(IvfHnswPqIndexBuilder::default()),
                )
                .execute()
                .await?;
        }

        let optimization_stats = table.optimize(OptimizeAction::All).await?;
        tracing::info!(
            "Retrieval table optimization stats:\n- Compaction: {:?} \n- Prune: {:?}\n",
            &optimization_stats.compaction,
            &optimization_stats.prune
        );

        Ok(())
    }
}
