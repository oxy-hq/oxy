use crate::{
    config::ConfigManager,
    errors::OxyError,
    semantic::{SemanticManager, types::SyncMetrics},
};

pub async fn sync_databases(
    config: ConfigManager,
    filter: Option<(String, Vec<String>)>,
    override_mode: bool,
) -> Result<Vec<Result<SyncMetrics, OxyError>>, OxyError> {
    SemanticManager::from_config(config, override_mode)
        .await?
        .sync_all(filter)
        .await
}
