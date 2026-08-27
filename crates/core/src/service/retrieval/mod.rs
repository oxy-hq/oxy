use crate::config::WorkingCopy;
use crate::{adapters::secrets::SecretsManager, config::ConfigManager};
use oxy_shared::errors::OxyError;

pub mod enum_index;
pub use enum_index::{EnumIndexConfig, EnumIndexManager};

pub struct ReindexInput {
    pub config: ConfigManager<WorkingCopy>,
    pub secrets_manager: SecretsManager,
    /// Retained for callers; the legacy LanceDB ingestion path is gone, so
    /// dropping tables no longer applies. Accepted for forward-compatible
    /// API shape.
    pub drop_all_tables: bool,
}

/// Rebuild the on-disk enum-index cache used by the agentic pipeline's
/// dimension-aware routing. The classic agent retrieval-tool index was
/// retired with the agent runner; only the enum index survives.
pub async fn reindex(input: ReindexInput) -> Result<(), OxyError> {
    let _ = input.drop_all_tables;
    if let Err(build_err) =
        EnumIndexManager::build_from_config(&input.config, &input.secrets_manager, &Vec::new())
            .await
    {
        tracing::warn!("Failed to build and persist enum index: {}", build_err);
    }
    Ok(())
}
