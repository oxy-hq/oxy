mod ingestion;
mod math;
mod schema;
mod search;
mod serialization;
mod table;

use lancedb::Connection;

use crate::{
    adapters::openai::OpenAIClient, config::model::EmbeddingConfig,
    service::retrieval::enum_index::EnumIndexManager,
};
use oxy_shared::errors::OxyError;

use super::{engine::VectorEngine, types::RetrievalObject};

use std::sync::Arc;

use ingestion::IngestionManager;
use search::SearchManager;
use table::TableManager;

pub(super) struct LanceDB {
    client: OpenAIClient,
    connection: Connection,
    embedding_config: EmbeddingConfig,
    table_manager: Arc<TableManager>,
    enum_index_manager: Arc<EnumIndexManager>,
}

impl LanceDB {
    pub(super) fn new(
        client: OpenAIClient,
        connection: Connection,
        embedding_config: EmbeddingConfig,
        enum_index_manager: Arc<EnumIndexManager>,
    ) -> Self {
        let table_manager = Arc::new(TableManager::new(
            connection.clone(),
            embedding_config.n_dims,
        ));

        Self {
            client,
            connection,
            embedding_config,
            table_manager,
            enum_index_manager,
        }
    }
}

impl VectorEngine for LanceDB {
    async fn ingest(&self, retrieval_objects: &Vec<RetrievalObject>) -> Result<(), OxyError> {
        let ingestion_manager = IngestionManager::new(
            self.client.clone(),
            self.embedding_config.clone(),
            self.table_manager.clone(),
        );
        // Build-time ingestion: reindex after upsert
        let reindex = true;
        ingestion_manager.ingest(retrieval_objects, reindex).await
    }

    async fn search(&self, query: &str) -> Result<Vec<super::types::SearchRecord>, OxyError> {
        let search_manager = SearchManager::new(
            self.embedding_config.clone(),
            self.client.clone(),
            self.table_manager.clone(),
            self.enum_index_manager.clone(),
        );
        search_manager.search(query).await
    }

    async fn cleanup(&self) -> Result<(), OxyError> {
        self.connection
            .drop_all_tables(&[])
            .await
            .map_err(OxyError::LanceDBError)
    }
}
