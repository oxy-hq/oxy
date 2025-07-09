mod embedding;
mod math;
mod schema;
mod search;
mod serialization;
mod table;

use lancedb::Connection;

use crate::{
    adapters::openai::OpenAIClient, 
    config::model::EmbeddingConfig, 
    errors::OxyError
};

use super::{
    engine::VectorEngine,
    types::{Document, SearchRecord},
};

use std::sync::Arc;

use embedding::EmbeddingManager;
use search::SearchManager;
use table::TableManager;

pub(super) struct LanceDB {
    client: OpenAIClient,
    connection: Connection,
    embedding_config: EmbeddingConfig,
    table_manager: Arc<TableManager>,
}

impl LanceDB {
    pub(super) fn new(
        client: OpenAIClient,
        connection: Connection,
        embedding_config: EmbeddingConfig,
    ) -> Self {
        let table_manager = Arc::new(TableManager::new(connection.clone(), embedding_config.n_dims));

        Self {
            client,
            connection,
            embedding_config,
            table_manager,
        }
    }
}

impl VectorEngine for LanceDB {
    async fn embed(&self, documents: &Vec<Document>) -> Result<(), OxyError> {
        let embedding_manager = EmbeddingManager::new(
            self.client.clone(),
            self.embedding_config.clone(),
            self.table_manager.clone(),
        );
        embedding_manager.embed(documents).await
    }

    async fn search(&self, query: &str) -> Result<Vec<SearchRecord>, OxyError> {
        let search_manager = SearchManager::new(
            self.embedding_config.clone(),
            self.client.clone(),
            self.table_manager.clone(),
        );
        search_manager.search(query).await
    }

    async fn cleanup(&self) -> Result<(), OxyError> {
        self.connection
            .drop_all_tables()
            .await
            .map_err(OxyError::LanceDBError)
    }
} 
