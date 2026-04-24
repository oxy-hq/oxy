use std::collections::HashMap;

use crate::{
    adapters::secrets::SecretsManager,
    config::ConfigManager,
    semantic::{SemanticManager, types::SyncMetrics},
};
use oxy_shared::errors::OxyError;

/// Filter for database sync operations.
#[derive(Debug, Clone, Default)]
pub struct SyncFilter {
    /// Database name to sync (e.g., "clickhouse"). If None, sync all databases.
    pub database: Option<String>,
    /// Dataset/schema names to include. Empty means all.
    pub datasets: Vec<String>,
    /// Specific tables to include, as "schema.table" strings.
    /// When non-empty, only these tables are synced (overrides datasets).
    pub tables: Vec<String>,
}

impl SyncFilter {
    /// Convert to the legacy filter tuple format (database, datasets).
    /// When `tables` is set, groups them by schema and returns table-level filter.
    pub fn into_filter(self) -> (Option<String>, HashMap<String, Vec<String>>) {
        if !self.tables.is_empty() {
            // Group "schema.table" entries by schema
            let mut schema_tables: HashMap<String, Vec<String>> = HashMap::new();
            for qualified in &self.tables {
                if let Some((schema, table)) = qualified.split_once('.') {
                    schema_tables
                        .entry(schema.to_string())
                        .or_default()
                        .push(table.to_string());
                }
            }
            (self.database, schema_tables)
        } else if !self.datasets.is_empty() {
            // Schema-level filter: all tables within listed schemas
            let datasets_map: HashMap<String, Vec<String>> = self
                .datasets
                .into_iter()
                .map(|d| (d, vec!["*".to_string()]))
                .collect();
            (self.database, datasets_map)
        } else {
            (self.database, HashMap::new())
        }
    }
}

pub async fn sync_databases(
    config: ConfigManager,
    secrets_manager: SecretsManager,
    filter: Option<SyncFilter>,
    overwrite: bool,
) -> Result<Vec<Result<SyncMetrics, OxyError>>, OxyError> {
    tracing::debug!(
        "sync_databases: Called with filter: {:?}, overwrite: {}",
        filter,
        overwrite
    );

    let semantic_manager = SemanticManager::from_config(config, secrets_manager, overwrite).await?;
    tracing::debug!("sync_databases: SemanticManager created successfully");

    let semantic_results = semantic_manager
        .sync_all(filter.unwrap_or_default())
        .await?;
    tracing::debug!(
        "sync_databases: sync_all completed with {} results",
        semantic_results.len()
    );

    // Convert semantic crate errors to core crate errors
    let results: Vec<Result<SyncMetrics, OxyError>> = semantic_results.into_iter().collect();

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
