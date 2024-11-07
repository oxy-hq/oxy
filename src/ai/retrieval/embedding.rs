use std::{cmp::min, sync::Arc};

use arrow::{
    array::{Array, FixedSizeListArray, RecordBatch, RecordBatchIterator, StringArray},
    datatypes::{DataType, Field, Float32Type, Schema},
};
use async_trait::async_trait;
use fastembed::{
    EmbeddingModel, InitOptions, RerankInitOptions, RerankerModel, TextEmbedding, TextRerank,
};
use futures::StreamExt;
use lancedb::{
    connect,
    connection::CreateTableMode,
    query::{ExecutableQuery, QueryBase},
    Connection, Table,
};
use serde::{Deserialize, Serialize};
use serde_arrow::from_record_batch;
use tokio::sync::OnceCell;

#[derive(Debug, Serialize, Deserialize)]
pub struct Document {
    pub content: String,
    pub source_type: String,
    pub source_identifier: String,
    pub embeddings: Vec<f32>,
}

#[async_trait]
pub trait VectorStore {
    async fn embed(&self, documents: &Vec<Document>) -> anyhow::Result<()>;
    async fn search(&self, query: &str) -> anyhow::Result<Vec<Document>>;
}

pub struct LanceDBStore {
    uri: String,
    connection: Arc<OnceCell<Connection>>,
    embed_model: TextEmbedding,
    rerank_model: TextRerank,
    n_dims: usize,
    top_k: usize,
    factor: usize,
}

impl LanceDBStore {
    pub fn new(
        uri: &str,
        embed_model: EmbeddingModel,
        rerank_model: RerankerModel,
        top_k: usize,
        factor: usize,
    ) -> Self {
        let connection_cell = Arc::new(tokio::sync::OnceCell::new());
        let connection_cell_clone = connection_cell.clone();
        let uri = uri.to_string();
        let uri_clone = uri.clone();

        tokio::spawn(async move {
            connection_cell_clone
                .get_or_init(|| async { Self::lazy_init(&uri).await })
                .await;
        });

        let embed_model_clone = &embed_model.clone();
        let embed_model_info = TextEmbedding::get_model_info(embed_model_clone).unwrap();
        let embed_model =
            TextEmbedding::try_new(InitOptions::new(embed_model).with_show_download_progress(true))
                .unwrap();

        let rerank_model = TextRerank::try_new(
            RerankInitOptions::new(rerank_model).with_show_download_progress(true),
        )
        .unwrap();
        Self {
            uri: uri_clone,
            connection: connection_cell,
            embed_model,
            rerank_model,
            n_dims: embed_model_info.dim,
            top_k,
            factor,
        }
    }

    async fn lazy_init(uri: &str) -> Connection {
        connect(uri).execute().await.unwrap()
    }

    async fn get_warehouse_metadata_table(&self) -> anyhow::Result<Table> {
        let connection = self
            .connection
            .get_or_init(|| async { Self::lazy_init(&self.uri).await })
            .await;
        let table_result = connection.open_table("warehouse_metadata").execute().await;
        let table = match table_result {
            Ok(table) => table,
            Err(_) => {
                let schema = Arc::new(Schema::new(vec![
                    Field::new("content", DataType::Utf8, false),
                    Field::new("source_type", DataType::Utf8, false),
                    Field::new("source_identifier", DataType::Utf8, false),
                    Field::new(
                        "embeddings",
                        DataType::FixedSizeList(
                            Arc::new(Field::new("item", DataType::Float32, true)),
                            self.n_dims.try_into().unwrap(),
                        ),
                        false,
                    ),
                ]));

                connection
                    .create_empty_table("warehouse_metadata", schema)
                    .mode(CreateTableMode::exist_ok(|builder| builder))
                    .execute()
                    .await?
            }
        };
        Ok(table)
    }
}

#[async_trait]
impl VectorStore for LanceDBStore {
    async fn embed(&self, documents: &Vec<Document>) -> anyhow::Result<()> {
        let table = self.get_warehouse_metadata_table().await?;
        let schema = Arc::new(Schema::new(vec![
            Field::new("content", DataType::Utf8, false),
            Field::new("source_type", DataType::Utf8, false),
            Field::new("source_identifier", DataType::Utf8, false),
            Field::new(
                "embeddings",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    self.n_dims.try_into().unwrap(),
                ),
                false,
            ),
        ]));
        let contents = Arc::new(StringArray::from_iter_values(
            documents.iter().map(|doc| doc.content.clone()),
        ));
        let source_types = Arc::new(StringArray::from_iter_values(
            documents.iter().map(|doc| doc.source_type.clone()),
        ));
        let source_identifiers = Arc::new(StringArray::from_iter_values(
            documents.iter().map(|doc| doc.source_identifier.clone()),
        ));

        let embedding_contents = documents
            .iter()
            .map(|doc| doc.content.clone())
            .collect::<Vec<String>>();
        let embedding_iter = self
            .embed_model
            .embed(embedding_contents, None)?
            .iter()
            .map(|v| Some(v.iter().map(|f| Some(f.to_owned())).collect::<Vec<_>>()))
            .collect::<Vec<_>>();

        let embeddings = Arc::new(
            FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                embedding_iter,
                self.n_dims.try_into().unwrap(),
            ),
        );
        log::info!("Total: {:?}", &embeddings.len());

        let batches = RecordBatchIterator::new(
            vec![RecordBatch::try_new(
                schema.clone(),
                vec![contents, source_types, source_identifiers, embeddings],
            )
            .unwrap()]
            .into_iter()
            .map(Ok),
            schema.clone(),
        );

        table.add(batches).execute().await?;
        log::info!("Embedded!");
        Ok(())
    }

    async fn search(&self, query: &str) -> anyhow::Result<Vec<Document>> {
        let query_vector = self.embed_model.embed(vec![query.to_string()], None)?;
        let table = self.get_warehouse_metadata_table().await?;
        let vector = query_vector.first().unwrap();
        let mut results = table
            .vector_search(vector.to_owned())?
            .limit(self.top_k * self.factor)
            .execute()
            .await?;

        let rb = results.next().await.unwrap()?;
        let docs: Vec<Document> = from_record_batch(&rb)?;

        log::info!("Reranking...");
        let documents = docs
            .iter()
            .map(|doc| doc.content.clone())
            .collect::<Vec<String>>();
        let results = self
            .rerank_model
            .rerank(query.to_string(), documents, true, None)?;
        let results = results.as_slice();
        let end = min(results.len(), self.top_k);
        let results = &results[0..end];
        for doc in results {
            let content = doc.document.as_ref().unwrap();
            log::info!(
                "Rank: {}\nScore: {}\nContent: {}",
                doc.index,
                doc.score,
                content
            );
            log::info!("-----------------");
        }
        Ok(docs)
    }
}
