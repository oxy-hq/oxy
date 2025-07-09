use arrow::array::RecordBatchReader;
use lancedb::{
    Connection, Table,
    database::CreateTableMode,
    index::{
        Index,
        scalar::{FtsIndexBuilder},
        vector::{IvfHnswPqIndexBuilder},
    },
    table::OptimizeAction,
};
use crate::errors::OxyError;
use crate::config::constants::{VECTOR_INDEX_MIN_ROWS, FTS_INDEX_MIN_ROWS, RETRIEVAL_INCLUSION_MIDPOINT_COLUMN};
use super::schema::SchemaUtils;

pub(super) struct TableManager {
    connection: Connection,
    n_dims: usize,
}

impl TableManager {
    pub(super) fn new(connection: Connection, n_dims: usize) -> Self {
        Self {
            connection,
            n_dims,
        }
    }

    pub(super) async fn get_or_create_table(&self, table_name: &str) -> Result<Table, OxyError> {
        let table_result = self.connection.open_table(table_name).execute().await;

        let expected_schema = SchemaUtils::create_expected_schema(self.n_dims);
        
        let table = match table_result {
            Ok(table) => {
                let existing_schema = table.schema().await?;
                
                if !SchemaUtils::schemas_match(&expected_schema, &existing_schema) {
                    drop(table);
                    self.connection.drop_table(table_name).await?;
                    
                    self.connection
                        .create_empty_table(table_name, expected_schema)
                        .mode(CreateTableMode::exist_ok(|builder| builder))
                        .execute()
                        .await?
                } else {
                    table
                }
            },
            Err(err) => match err {
                lancedb::Error::TableNotFound { name } => {
                    self.connection
                        .create_empty_table(name, expected_schema)
                        .mode(CreateTableMode::exist_ok(|builder| builder))
                        .execute()
                        .await?
                }
                _ => {
                    return Err(err.into());
                },
            },
        };
        Ok(table)
    }

    pub(super) async fn add_batches(
        &self,
        table: &Table,
        batches: Box<dyn RecordBatchReader + Send>,
    ) -> anyhow::Result<()> {
        let mut merge_insert_op = table.merge_insert(&["source_identifier"]);
        merge_insert_op
            .when_matched_update_all(None)
            .when_not_matched_insert_all();
        merge_insert_op.execute(Box::new(batches)).await?;
        
        let indices = table.list_indices().await?;
        let num_rows = table.count_rows(None).await?;
        
        let fts_index = indices
            .iter()
            .find(|index| index.columns == vec!["content"]);

        // TODO: this index is currently not used, but we may want to do hybrid FTS + vector search
        if fts_index.is_none() && num_rows >= FTS_INDEX_MIN_ROWS {
            table
                .create_index(
                    &["content"],
                    Index::FTS(FtsIndexBuilder::default()),
                )
                .execute()
                .await?;
        }

        let vector_index = indices
            .iter()
            .find(|index| index.columns.contains(&RETRIEVAL_INCLUSION_MIDPOINT_COLUMN.to_string()));

        if vector_index.is_none() && num_rows >= VECTOR_INDEX_MIN_ROWS {
            table
                .create_index(
                    &[RETRIEVAL_INCLUSION_MIDPOINT_COLUMN],
                    Index::IvfHnswPq(IvfHnswPqIndexBuilder::default()),
                )
                .execute()
                .await?;
        }

        let optimization_stats = table.optimize(OptimizeAction::All).await?;
        tracing::info!(
            "Optimization stats:\n- Compaction: {:?} \n- Prune: {:?}\n",
            &optimization_stats.compaction,
            &optimization_stats.prune
        );
        Ok(())
    }
} 
