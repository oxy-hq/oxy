use std::sync::Arc;

use arrow::{
    array::{
        Array, FixedSizeListArray, RecordBatch, RecordBatchIterator, RecordBatchReader, StringArray,
    },
    datatypes::{DataType, Field, Float32Type, Schema},
};
use async_openai::{
    Client,
    types::{CreateEmbeddingRequestArgs, EmbeddingInput},
};
use lancedb::{
    Connection, Table, connect,
    database::CreateTableMode,
    index::{
        Index,
        scalar::{FtsIndexBuilder, FullTextSearchQuery},
        vector::IvfHnswPqIndexBuilder,
    },
    query::{ExecutableQuery, QueryBase},
    table::OptimizeAction,
};
use serde::{Deserialize, Serialize};
use serde_arrow::from_record_batch;

use crate::{
    adapters::reranking::ReciprocalRankingFusion,
    config::model::{EmbeddingConfig, RetrievalConfig, VectorDBConfig},
    errors::OxyError,
};

use super::openai::OpenAIClient;

#[derive(Debug, Serialize, Deserialize)]
pub struct Document {
    pub content: String,
    pub source_type: String,
    pub source_identifier: String,
    pub embeddings: Vec<f32>,
    pub embedding_content: String,
}

pub struct VectorStore {
    inner: VectorStoreImpl,
}

impl VectorStore {
    pub async fn from_retrieval(retrieval: &RetrievalConfig) -> Result<Self, OxyError> {
        match &retrieval.db_config {
            VectorDBConfig::LanceDB { db_path } => {
                let client = Client::with_config(retrieval.try_into()?);
                let connection = connect(db_path)
                    .execute()
                    .await
                    .map_err(OxyError::LanceDBError)?;
                Ok(Self {
                    inner: VectorStoreImpl::lancedb(
                        client,
                        connection,
                        retrieval.embedding_config.clone(),
                    ),
                })
            }
        }
    }
    pub async fn embed(&self, documents: &Vec<Document>) -> Result<(), OxyError> {
        self.inner.embed(documents).await
    }
    pub async fn search(&self, query: &str) -> Result<Vec<Document>, OxyError> {
        self.inner.search(query).await
    }
}

#[enum_dispatch::enum_dispatch]
pub(super) trait VectorEngine {
    async fn embed(&self, documents: &Vec<Document>) -> Result<(), OxyError>;
    async fn search(&self, query: &str) -> Result<Vec<Document>, OxyError>;
}

#[enum_dispatch::enum_dispatch(VectorEngine)]
pub(super) enum VectorStoreImpl {
    LanceDB,
}

impl VectorStoreImpl {
    pub(super) fn lancedb(
        client: OpenAIClient,
        connection: Connection,
        embedding_config: EmbeddingConfig,
    ) -> Self {
        VectorStoreImpl::LanceDB(LanceDB::new(client, connection, embedding_config))
    }
}

struct LanceDB {
    client: OpenAIClient,
    connection: Connection,
    embedding_config: EmbeddingConfig,
}

impl LanceDB {
    fn new(
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
        log::info!(
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
        log::info!("Total: {:?}", &embeddings.len());

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
        log::info!("{} documents embedded!", documents.len());
        Ok(())
    }

    async fn search(&self, query: &str) -> Result<Vec<Document>, OxyError> {
        log::info!("Embedding search query: {}", query);
        let query_vector = self.embed_query(query).await?;

        if query_vector.is_empty() {
            return Err(OxyError::RuntimeError(
                "Failed to generate embeddings for query".into(),
            ));
        }

        let table = self
            .get_or_create_table(&self.embedding_config.table)
            .await?;
        let mut results = table
            .vector_search(query_vector)?
            .limit(self.embedding_config.top_k * self.embedding_config.factor)
            .with_row_id()
            .execute()
            .await?;
        let mut fts_results = table
            .query()
            .full_text_search(FullTextSearchQuery::new(query.to_string()))
            .limit(self.embedding_config.top_k * self.embedding_config.factor)
            .with_row_id()
            .execute()
            .await?;

        let record_batch = ReciprocalRankingFusion::default()
            .rerank(
                &mut results,
                &mut fts_results,
                Some(self.embedding_config.top_k),
            )
            .await?;
        let docs: Vec<Document> =
            from_record_batch(&record_batch).map_err(OxyError::SerdeArrowError)?;
        Ok(docs)
    }
}
