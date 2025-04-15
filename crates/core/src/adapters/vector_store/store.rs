use std::path::PathBuf;

use super::{
    engine::VectorEngine,
    lance_db::LanceDB,
    types::{Document, SearchRecord},
};
use crate::{
    adapters::openai::OpenAIClient,
    config::{
        ConfigManager,
        model::{EmbeddingConfig, RetrievalConfig, VectorDBConfig},
    },
    errors::OxyError,
};
use lancedb::{Connection, connect};

#[enum_dispatch::enum_dispatch(VectorEngine)]
enum VectorStoreImpl {
    LanceDB,
}

impl VectorStoreImpl {
    fn lance_db(
        client: OpenAIClient,
        connection: Connection,
        embedding_config: EmbeddingConfig,
    ) -> Self {
        VectorStoreImpl::LanceDB(LanceDB::new(client, connection, embedding_config))
    }
}

pub struct VectorStore {
    inner: VectorStoreImpl,
}

impl VectorStore {
    pub async fn from_retrieval(
        config_manager: &ConfigManager,
        agent_name: &str,
        retrieval: &RetrievalConfig,
    ) -> Result<Self, OxyError> {
        match &retrieval.db_config {
            VectorDBConfig::LanceDB { db_path } => {
                let path = config_manager.resolve_file(db_path).await?;
                let db_path = PathBuf::from(&path)
                    .join(format!("{}-{}", agent_name, retrieval.name))
                    .to_string_lossy()
                    .to_string();
                let client = OpenAIClient::with_config(retrieval.try_into()?);
                let connection = connect(&db_path)
                    .execute()
                    .await
                    .map_err(OxyError::LanceDBError)?;
                Ok(Self {
                    inner: VectorStoreImpl::lance_db(
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
    pub async fn search(&self, query: &str) -> Result<Vec<SearchRecord>, OxyError> {
        self.inner.search(query).await
    }
}
