use crate::{
    adapters::secrets::SecretsManager,
    config::{
        ConfigManager,
        model::{AgentType, ToolType},
    },
};
use oxy_shared::errors::OxyError;

use super::{VectorStore, types::SearchRecord};

pub async fn search_agent(
    config: &ConfigManager,
    secrets_manager: &SecretsManager,
    agent_ref: &str,
    query: &str,
) -> Result<Vec<SearchRecord>, OxyError> {
    let agent = config.resolve_agent(agent_ref).await?;
    let results = match &agent.r#type {
        AgentType::Default(default_agent) => {
            let mut results = vec![];
            for retrieval in &default_agent.tools_config.tools {
                if let ToolType::Retrieval(retrieval) = retrieval {
                    tracing::info!(agent = %agent.name, tool = %retrieval.name, "Searching using agent tool");
                    let vector_store = VectorStore::from_retrieval(
                        config,
                        secrets_manager,
                        &agent.name,
                        retrieval,
                    )
                    .await?;
                    let documents = vector_store.search(query).await?;
                    for document in documents.iter() {
                        tracing::debug!(document = ?document, "Search result");
                    }
                    results.extend(documents);
                }
            }
            results
        }
        AgentType::Routing(routing_agent) => {
            tracing::info!(agent = %agent.name, "Searching using routing agent");
            let vector_store = VectorStore::from_routing_agent(
                config,
                secrets_manager,
                &agent.name,
                &agent.model,
                routing_agent,
            )
            .await?;
            let documents = vector_store.search(query).await?;
            for document in documents.iter() {
                tracing::debug!(document = ?document, "Search result");
            }
            documents
        }
    };

    Ok(results)
}
