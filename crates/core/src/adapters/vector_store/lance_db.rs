use std::sync::Arc;

use arrow::{
    array::{
        Array, FixedSizeListArray, RecordBatch, RecordBatchIterator, RecordBatchReader, StringArray,
    },
    datatypes::{DataType, Field, Float32Type, Schema},
};
use async_openai::types::{CreateEmbeddingRequestArgs, EmbeddingInput};
use futures::TryStreamExt;
use lancedb::{
    Connection, Table,
    database::CreateTableMode,
    index::{
        Index,
        scalar::{FtsIndexBuilder, FullTextSearchQuery},
        vector::IvfHnswPqIndexBuilder,
    },
    query::{ExecutableQuery, QueryBase},
    table::OptimizeAction,
};
use serde_arrow::from_record_batch;

use crate::{adapters::openai::OpenAIClient, config::model::EmbeddingConfig, errors::OxyError};

use super::{
    engine::VectorEngine,
    types::{Document, SearchRecord},
};

pub(super) struct LanceDB {
    client: OpenAIClient,
    connection: Connection,
    embedding_config: EmbeddingConfig,
}

impl LanceDB {
    pub(super) fn new(
        client: OpenAIClient,
        connection: Connection,
        embedding_config: EmbeddingConfig,
    ) -> Self {
        Self {
            client,
            connection,
            embedding_config,
        }
    }

    async fn get_or_create_table(&self, table_name: &str) -> Result<Table, OxyError> {
        let table_result = self.connection.open_table(table_name).execute().await;
        let table = match table_result {
            Ok(table) => table,
            Err(err) => match err {
                lancedb::Error::TableNotFound { name } => {
                    let schema = Arc::new(Schema::new(vec![
                        Field::new("content", DataType::Utf8, false),
                        Field::new("source_type", DataType::Utf8, false),
                        Field::new("source_identifier", DataType::Utf8, false),
                        Field::new("embedding_content", DataType::Utf8, false),
                        Field::new(
                            "embeddings",
                            DataType::FixedSizeList(
                                Arc::new(Field::new("item", DataType::Float32, true)),
                                self.embedding_config.n_dims.try_into().unwrap(),
                            ),
                            false,
                        ),
                    ]));

                    self.connection
                        .create_empty_table(name, schema)
                        .mode(CreateTableMode::exist_ok(|builder| builder))
                        .execute()
                        .await?
                }
                _ => return Err(err.into()),
            },
        };
        Ok(table)
    }

    async fn add_batches(
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
        let fts_index = indices
            .iter()
            .find(|index| index.columns == vec!["embedding_content"]);
        let vector_index = indices
            .iter()
            .find(|index| index.columns == vec!["embeddings"]);

        if fts_index.is_none() {
            table
                .create_index(
                    &["embedding_content"],
                    Index::FTS(FtsIndexBuilder::default()),
                )
                .execute()
                .await?;
        }

        if vector_index.is_none() {
            let num_rows = table.count_rows(None).await?;
            if num_rows >= 256 {
                table
                    .create_index(
                        &["embeddings"],
                        Index::IvfHnswPq(IvfHnswPqIndexBuilder::default()),
                    )
                    .execute()
                    .await?;
            }
        }

        let optimization_stats = table.optimize(OptimizeAction::All).await?;
        tracing::info!(
            "Optimization stats:\n- Compaction: {:?} \n- Prune: {:?}\n",
            &optimization_stats.compaction,
            &optimization_stats.prune
        );
        Ok(())
    }

    async fn embed_query(&self, query: &str) -> anyhow::Result<Vec<f32>> {
        let embeddings_request = CreateEmbeddingRequestArgs::default()
            .model(self.embedding_config.embed_model.clone())
            .input(EmbeddingInput::String(query.to_string()))
            .dimensions(self.embedding_config.n_dims as u32)
            .build()?;
        let embeddings_response = self.client.embeddings().create(embeddings_request).await?;
        Ok(embeddings_response.data[0].embedding.clone())
    }

    async fn embed_documents(
        &self,
        documents: &Vec<Document>,
    ) -> anyhow::Result<Vec<Option<Vec<Option<f32>>>>> {
        let embedding_contents = documents
            .iter()
            .map(|doc| doc.embedding_content.clone())
            .collect::<Vec<String>>();
        let embeddings_request = CreateEmbeddingRequestArgs::default()
            .model(self.embedding_config.embed_model.clone())
            .input(EmbeddingInput::StringArray(embedding_contents))
            .dimensions(self.embedding_config.n_dims as u32)
            .build()?;
        let embeddings_response = self.client.embeddings().create(embeddings_request).await?;
        Ok(embeddings_response
            .data
            .iter()
            .map(|e| Some(e.embedding.iter().map(|v| Some(v.to_owned())).collect()))
            .collect())
    }
}

impl VectorEngine for LanceDB {
    async fn embed(&self, documents: &Vec<Document>) -> Result<(), OxyError> {
        let table = self
            .get_or_create_table(&self.embedding_config.table)
            .await?;
        let schema = table.schema().await?;
        let contents = Arc::new(StringArray::from_iter_values(
            documents.iter().map(|doc| doc.content.clone()),
        ));
        let source_types = Arc::new(StringArray::from_iter_values(
            documents.iter().map(|doc| doc.source_type.clone()),
        ));
        let source_identifiers = Arc::new(StringArray::from_iter_values(
            documents.iter().map(|doc| doc.source_identifier.clone()),
        ));

        let embedding_contents = Arc::new(StringArray::from_iter_values(
            documents.iter().map(|doc| doc.embedding_content.clone()),
        ));
        let embedding_iter = self.embed_documents(documents).await?;

        let embeddings: Arc<FixedSizeListArray> = Arc::new(
            FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                embedding_iter,
                self.embedding_config.n_dims.try_into().unwrap(),
            ),
        );
        tracing::info!("Total embedding records: {:?}", &embeddings.len());

        // clean up the table
        table.delete("true").await?;

        // insert new data
        let batches = RecordBatchIterator::new(
            vec![
                RecordBatch::try_new(
                    schema.clone(),
                    vec![
                        contents,
                        source_types,
                        source_identifiers,
                        embedding_contents,
                        embeddings,
                    ],
                )
                .unwrap(),
            ]
            .into_iter()
            .map(Ok),
            schema.clone(),
        );
        self.add_batches(&table, Box::new(batches)).await?;
        tracing::info!("{} documents embedded!", documents.len());
        Ok(())
    }

    async fn search(&self, query: &str) -> Result<Vec<SearchRecord>, OxyError> {
        tracing::info!("Embedding search query: {}", query);
        let query_vector = self.embed_query(query).await?;

        if query_vector.is_empty() {
            return Err(OxyError::RuntimeError(
                "Failed to generate embeddings for query".into(),
            ));
        }

        let table = self
            .get_or_create_table(&self.embedding_config.table)
            .await?;
        let stream = table
            .query()
            .full_text_search(FullTextSearchQuery::new(query.to_string()))
            .limit(self.embedding_config.top_k * self.embedding_config.factor)
            .nearest_to(query_vector)?
            .execute()
            .await?;

        tracing::debug!("Query results schema: {:?}", stream.schema());

        let record_batches = stream.try_collect::<Vec<_>>().await?;
        let mut results = vec![];
        for record_batch in record_batches {
            let docs: Vec<SearchRecord> =
                from_record_batch(&record_batch).map_err(OxyError::SerdeArrowError)?;
            results.extend(docs);
        }
        Ok(results)
    }

    async fn cleanup(&self) -> Result<(), OxyError> {
        self.connection
            .drop_all_tables()
            .await
            .map_err(OxyError::LanceDBError)
    }
}
