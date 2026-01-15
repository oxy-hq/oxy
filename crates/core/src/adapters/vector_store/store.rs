use std::{path::PathBuf, sync::Arc};

use super::{
    engine::VectorEngine,
    lance_db::LanceDB,
    types::{RetrievalObject, SearchRecord},
};
use crate::{
    adapters::{
        openai::{IntoOpenAIConfig, OpenAIClient},
        secrets::SecretsManager,
    },
    config::{
        ConfigManager,
        model::{EmbeddingConfig, RetrievalConfig, RoutingAgent, VectorDBConfig},
    },
    service::retrieval::EnumIndexManager,
};
use enum_dispatch::enum_dispatch;
use lancedb::{Connection, connect};
use oxy_shared::errors::OxyError;

#[enum_dispatch(VectorEngine)]
enum VectorStoreImpl {
    LanceDB,
}

impl VectorStoreImpl {
    fn lance_db(
        client: OpenAIClient,
        connection: Connection,
        embedding_config: EmbeddingConfig,
        enum_index_manager: Arc<EnumIndexManager>,
    ) -> Self {
        VectorStoreImpl::LanceDB(LanceDB::new(
            client,
            connection,
            embedding_config,
            enum_index_manager,
        ))
    }
}

pub struct VectorStore {
    inner: VectorStoreImpl,
}

impl VectorStore {
    pub async fn new(
        config_manager: &ConfigManager,
        secrets_manager: &SecretsManager,
        db_config: &VectorDBConfig,
        name: &str,
        openai_config: impl IntoOpenAIConfig,
        embedding_config: EmbeddingConfig,
    ) -> Result<Self, OxyError> {
        let client =
            OpenAIClient::with_config(openai_config.into_openai_config(secrets_manager).await?);
        // Create minimal enum index config for VectorStore (main enum index is managed at higher level)
        let enum_index_manager = Arc::new(EnumIndexManager::from_config(config_manager).await?);
        let connection = match &db_config {
            VectorDBConfig::LanceDB { db_path } => {
                let resolved_root = config_manager.resolve_file(db_path).await?;
                let db_path = PathBuf::from(&resolved_root)
                    .join(name)
                    .to_string_lossy()
                    .to_string();

                connect(&db_path)
                    .execute()
                    .await
                    .map_err(OxyError::LanceDBError)?
            }
        };
        Ok(Self {
            inner: VectorStoreImpl::lance_db(
                client,
                connection,
                embedding_config,
                enum_index_manager,
            ),
        })
    }
    pub async fn from_retrieval(
        config_manager: &ConfigManager,
        secrets_manager: &SecretsManager,
        agent_name: &str,
        retrieval: &RetrievalConfig,
    ) -> Result<Self, OxyError> {
        VectorStore::new(
            config_manager,
            secrets_manager,
            &retrieval.db_config,
            &format!("{}-{}", agent_name, retrieval.name),
            retrieval.clone(),
            retrieval.embedding_config.clone(),
        )
        .await
    }
    pub async fn from_routing_agent(
        config_manager: &ConfigManager,
        secrets_manager: &SecretsManager,
        agent_name: &str,
        model: &str,
        routing_agent: &RoutingAgent,
    ) -> Result<Self, OxyError> {
        let model = config_manager.resolve_model(model)?;
        VectorStore::new(
            config_manager,
            secrets_manager,
            &routing_agent.db_config,
            &format!("{agent_name}-routing"),
            model.clone(),
            routing_agent.embedding_config.clone(),
        )
        .await
    }
    pub async fn ingest(&self, retrieval_objects: &Vec<RetrievalObject>) -> Result<(), OxyError> {
        self.inner.ingest(retrieval_objects).await
    }
    pub async fn search(&self, query: &str) -> Result<Vec<SearchRecord>, OxyError> {
        self.inner.search(query).await
    }
    pub async fn cleanup(&self) -> Result<(), OxyError> {
        self.inner.cleanup().await
    }
}
