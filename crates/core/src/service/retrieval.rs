use crate::{
    adapters::vector_store::{SearchRecord, reindex_all, search_agent},
    config::ConfigBuilder,
    errors::OxyError,
};

pub struct ReindexInput {
    pub project_path: String,
    pub drop_all_tables: bool,
}

pub async fn reindex(input: ReindexInput) -> Result<(), OxyError> {
    let config = ConfigBuilder::new()
        .with_project_path(input.project_path)?
        .build()
        .await?;
    reindex_all(&config, input.drop_all_tables).await
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
