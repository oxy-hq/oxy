use crate::{
    adapters::{
        secrets::SecretsManager,
        vector_store::{
            SearchRecord, build_all_retrieval_objects, ingest_retrieval_objects, search_agent,
        },
    },
    config::ConfigManager,
    errors::OxyError,
};

pub mod enum_index;
pub use enum_index::{EnumIndexConfig, EnumIndexManager};

pub struct ReindexInput {
    pub config: ConfigManager,
    pub secrets_manager: SecretsManager,
    pub drop_all_tables: bool,
}

pub async fn reindex(input: ReindexInput) -> Result<(), OxyError> {
    // Build all retrieval objects once
    let retrieval_objects = build_all_retrieval_objects(&input.config).await?;

    // Build enum index with the retrieval objects
    if let Err(build_err) = EnumIndexManager::build_from_config(
        &input.config,
        &input.secrets_manager,
        &retrieval_objects,
    )
    .await
    {
        tracing::warn!("Failed to build and persist enum index: {}", build_err);
    }

    // Ingest the retrieval objects
    ingest_retrieval_objects(
        &input.config,
        &input.secrets_manager,
        &retrieval_objects,
        input.drop_all_tables,
    )
    .await
}

pub struct SearchInput {
    pub config: ConfigManager,
    pub secrets_manager: SecretsManager,
    pub agent_ref: String,
    pub query: String,
}

pub async fn search(input: SearchInput) -> Result<Vec<SearchRecord>, OxyError> {
    search_agent(
        &input.config,
        &input.secrets_manager,
        &input.agent_ref,
        &input.query,
    )
    .await
}
