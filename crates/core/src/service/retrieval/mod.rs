use crate::{
    adapters::vector_store::{SearchRecord, build_all_retrieval_objects, ingest_retrieval_objects, search_agent},
    config::ConfigBuilder,
    errors::OxyError,
};

pub mod enum_index;
pub use enum_index::{EnumIndexConfig, EnumIndexManager};

pub struct ReindexInput {
    pub project_path: String,
    pub drop_all_tables: bool,
}

pub async fn reindex(input: ReindexInput) -> Result<(), OxyError> {
    let config = ConfigBuilder::new()
        .with_project_path(input.project_path)?
        .build()
        .await?;

    // Build all retrieval objects once
    let retrieval_objects = build_all_retrieval_objects(&config).await?;

    // Build enum index with the retrieval objects
    if let Err(build_err) = EnumIndexManager::build_from_config(&config, &retrieval_objects).await {
        tracing::warn!("Failed to build and persist enum index: {}", build_err);
    }

    // Ingest the retrieval objects
    ingest_retrieval_objects(&config, &retrieval_objects, input.drop_all_tables).await
}

pub struct SearchInput {
    pub project_path: String,
    pub agent_ref: String,
    pub query: String,
}

pub async fn search(input: SearchInput) -> Result<Vec<SearchRecord>, OxyError> {
    let config = ConfigBuilder::new()
        .with_project_path(input.project_path)?
        .build()
        .await?;
    search_agent(&config, &input.agent_ref, &input.query).await
}
