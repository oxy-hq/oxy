use std::path::PathBuf;

use super::{
    engine::VectorEngine,
    lance_db::LanceDB,
    types::{Document, SearchRecord},
};
use crate::{
    adapters::openai::{ConfigType, OpenAIClient},
    config::{
        ConfigManager,
        model::{EmbeddingConfig, RetrievalConfig, RoutingAgent, VectorDBConfig},
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
    pub async fn new(
        config_manager: &ConfigManager,
        db_config: &VectorDBConfig,
        name: &str,
        openai_config: impl TryInto<ConfigType, Error = OxyError>,
        embedding_config: EmbeddingConfig,
    ) -> Result<Self, OxyError> {
        let client = OpenAIClient::with_config(openai_config.try_into()?);
        let connection = match &db_config {
            VectorDBConfig::LanceDB { db_path } => {
                let path = config_manager.resolve_file(db_path).await?;
                let db_path = PathBuf::from(&path)
                    .join(name)
                    .to_string_lossy()
                    .to_string();
                connect(&db_path)
                    .execute()
                    .await
                    .map_err(OxyError::LanceDBError)
            }
        }?;
        Ok(Self {
            inner: VectorStoreImpl::lance_db(client, connection, embedding_config),
        })
    }
    pub async fn from_retrieval(
        config_manager: &ConfigManager,
        agent_name: &str,
        retrieval: &RetrievalConfig,
    ) -> Result<Self, OxyError> {
        VectorStore::new(
            config_manager,
            &retrieval.db_config,
            &format!("{}-{}", agent_name, retrieval.name),
            retrieval,
            retrieval.embedding_config.clone(),
        )
        .await
    }
    pub async fn from_routing_agent(
        config_manager: &ConfigManager,
        agent_name: &str,
        model: &str,
        routing_agent: &RoutingAgent,
    ) -> Result<Self, OxyError> {
        let model = config_manager.resolve_model(model)?;
        VectorStore::new(
            config_manager,
            &routing_agent.db_config,
            &format!("{}-routing", agent_name),
            model,
            routing_agent.embedding_config.clone(),
        )
        .await
    }
    pub async fn embed(&self, documents: &Vec<Document>) -> Result<(), OxyError> {
        self.inner.embed(documents).await
    }
    pub async fn search(&self, query: &str) -> Result<Vec<SearchRecord>, OxyError> {
        self.inner.search(query).await
    }
    pub async fn cleanup(&self) -> Result<(), OxyError> {
        self.inner.cleanup().await
    }
}
