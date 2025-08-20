use crate::{
    config::ConfigManager,
    errors::OxyError,
    semantic::{SemanticManager, types::SyncMetrics},
};

pub async fn sync_databases(
    config: ConfigManager,
    filter: Option<(String, Vec<String>)>,
    overwrite: bool,
) -> Result<Vec<Result<SyncMetrics, OxyError>>, OxyError> {
    tracing::info!("🎯 sync_databases: Called with filter: {:?}, overwrite: {}", filter, overwrite);
    eprintln!("🎯 DEBUG: sync_databases called with filter: {:?}", filter);
    
    let semantic_manager = SemanticManager::from_config(config, overwrite).await?;
    tracing::info!("🎯 sync_databases: SemanticManager created successfully");
    
    let results = semantic_manager.sync_all(filter).await?;
    tracing::info!("🎯 sync_databases: sync_all completed with {} results", results.len());

    let would_overwrite_count: usize = results
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .map(|metrics| metrics.would_overwrite_files.len())
        .sum();

    let overwritten_count: usize = results
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .map(|metrics| metrics.overwritten_files.len())
        .sum();

    let deleted_count: usize = results
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .map(|metrics| metrics.deleted_files.len())
        .sum();

    let created_count: usize = results
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .map(|metrics| metrics.created_files.len())
        .sum();

    if would_overwrite_count > 0 {
        tracing::warn!(
            "Total of {} files would be overwritten. Use --overwrite flag to force overwriting of existing files.",
            would_overwrite_count
        );
    }

    if overwritten_count > 0 {
        tracing::warn!(
            "Total of {} files were overwritten because --overwrite flag was used.",
            overwritten_count
        );
    }

    if deleted_count > 0 {
        tracing::warn!(
            "Total of {} files were deleted (not in output).",
            deleted_count
        );
    }

    if created_count > 0 {
        tracing::info!("Total of {} new files were created.", created_count);
    }

    Ok(results)
}
