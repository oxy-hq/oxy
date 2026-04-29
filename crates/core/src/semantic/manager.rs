use itertools::Itertools;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};

use futures::StreamExt;
use tqdm::{Tqdm, pbar};

use crate::{
    adapters::secrets::SecretsManager,
    config::{ConfigManager, model::Database},
    semantic::{loader::SchemaLoader, types::SemanticKey},
};
use oxy_shared::errors::OxyError;

use super::{
    SemanticVariablesContexts,
    contexts::SemanticDimensionsContexts,
    storage::SemanticStorage,
    types::{DatabaseInfo, SyncMetrics},
};

pub struct SemanticManager {
    config: ConfigManager,
    secrets_manager: SecretsManager,
    storage: SemanticStorage,
}

impl SemanticManager {
    pub async fn from_config(
        config: ConfigManager,
        secrets_manager: SecretsManager,
        override_mode: bool,
    ) -> Result<Self, OxyError> {
        let storage = SemanticStorage::from_config(&config, override_mode).await?;
        Ok(SemanticManager {
            config,
            secrets_manager,
            storage,
        })
    }

    pub async fn load_database_info(&self, database_ref: &str) -> Result<DatabaseInfo, OxyError> {
        let database = self.config.resolve_database(database_ref)?;
        let datasets = match self.storage.load_datasets(database_ref).await {
            Ok(datasets) => datasets,
            Err(err) => match err {
                OxyError::IOError(_) => {
                    self.sync(&database, None).await?;
                    self.storage.load_datasets(database_ref).await?
                }
                _ => {
                    return Err(err);
                }
            },
        };
        Ok(DatabaseInfo {
            name: database_ref.to_string(),
            dialect: database.dialect(),
            datasets: datasets
                .into_iter()
                .map(|d| (d.dataset.clone(), d))
                .collect(),
        })
    }

    /// Try to load database info from cache only, without triggering sync.
    /// Returns None if the database hasn't been synced yet.
    pub async fn try_load_cached_database_info(
        &self,
        database_ref: &str,
    ) -> Result<Option<DatabaseInfo>, OxyError> {
        let database = self.config.resolve_database(database_ref)?;
        match self.storage.load_datasets(database_ref).await {
            Ok(datasets) => Ok(Some(DatabaseInfo {
                name: database_ref.to_string(),
                dialect: database.dialect(),
                datasets: datasets
                    .into_iter()
                    .map(|d| (d.dataset.clone(), d))
                    .collect(),
            })),
            Err(OxyError::IOError(_)) => {
                // Not synced yet, return None
                Ok(None)
            }
            Err(err) => Err(err),
        }
    }

    /// Build an empty `SemanticVariablesContexts` for jinja template rendering.
    ///
    /// Previously this enumerated `dimensions[].targets` from `semantics.yml` and
    /// hydrated per-table dimension schemas. With the global semantics registry
    /// removed, callers get an empty context — `{{ models.<table>.* }}` template
    /// references resolve to undefined.
    pub async fn get_semantic_variables_contexts(
        &self,
    ) -> Result<SemanticVariablesContexts, OxyError> {
        SemanticVariablesContexts::new(HashMap::new())
    }

    /// Build an empty `SemanticDimensionsContexts` for jinja template rendering.
    ///
    /// Previously this exposed dimensions declared in `semantics.yml`. With the
    /// global semantics registry removed, callers get an empty context.
    pub async fn get_semantic_dimensions_contexts(
        &self,
    ) -> Result<SemanticDimensionsContexts, OxyError> {
        Ok(SemanticDimensionsContexts {
            dimensions: HashMap::new(),
        })
    }

    async fn sync(
        &self,
        database: &Database,
        pbar: Option<Arc<Mutex<Tqdm<()>>>>,
    ) -> Result<SyncMetrics, OxyError> {
        let start_time = Instant::now();

        tracing::debug!(
            "Starting sync for database: {} (type: {:?})",
            database.name,
            database.database_type
        );

        let loader =
            SchemaLoader::from_database(database, &self.config, &self.secrets_manager).await?;
        tracing::debug!(
            "SchemaLoader created successfully for database: {}",
            database.name
        );

        tracing::debug!("Loading schema for database: {}", database.name);
        let semantics = match loader.load_schema(&self.config).await {
            Ok(semantics) => {
                tracing::debug!(
                    "Schema loaded for database: {} - found {} datasets",
                    database.name,
                    semantics.len()
                );
                for (dataset, models) in &semantics {
                    tracing::trace!("Dataset '{}' has {} models", dataset, models.len());
                    for (table_name, model) in models {
                        tracing::trace!(
                            "Table '{}' has {} dimensions",
                            table_name,
                            model.dimensions.len()
                        );
                    }
                }
                semantics
            }
            Err(e) => {
                tracing::error!(
                    "Failed to load schema for database {}: {}",
                    database.name,
                    e
                );
                return Err(e);
            }
        };
        let mut output_files = vec![];
        let mut would_overwrite_files: Vec<String> = vec![];
        let mut overwritten_files = vec![];
        let mut deleted_files = vec![];
        let mut created_files = vec![];
        let mut dimensions = vec![];

        for (dataset, semantic_models) in semantics {
            let key = SemanticKey::new(database.name.to_string(), dataset);
            let result = self.storage.save_model(&key, semantic_models).await?;
            output_files.push(result.base_path.clone());

            would_overwrite_files.extend(result.would_overwrite_files.clone());
            overwritten_files.extend(result.overwritten_files.clone());
            created_files.extend(result.created_files.clone());
            deleted_files.extend(result.deleted_files.clone());
            dimensions.extend_from_slice(result.dimensions.as_slice());
        }

        let ddls = loader.load_ddl(&self.config).await?;
        for (dataset, ddl) in ddls {
            let key = SemanticKey::new(database.name.to_string(), dataset);
            let result = self.storage.save_ddl(&key, ddl).await?;
            output_files.push(result.base_path.clone());

            would_overwrite_files.extend(result.would_overwrite_files.clone());
            overwritten_files.extend(result.overwritten_files.clone());
            created_files.extend(result.created_files.clone());
            deleted_files.extend(result.deleted_files.clone());
        }

        if let Some(pbar) = pbar {
            pbar.lock().unwrap().update(1)?;
        }

        if !would_overwrite_files.is_empty() {
            tracing::warn!(
                "Some files were skipped because they already exist: \n{}",
                would_overwrite_files
                    .iter()
                    .map(|f| format!("- {f}"))
                    .join("\n")
            );
        }

        if !overwritten_files.is_empty() {
            tracing::warn!(
                "Some existing files were overwritten (--overwrite flag was used): \n{}",
                overwritten_files
                    .iter()
                    .map(|f| format!("- {f}"))
                    .join("\n")
            );
        }

        if !deleted_files.is_empty() {
            tracing::warn!(
                "Some files were deleted (not in output): \n{}",
                deleted_files.iter().map(|f| format!("- {f}")).join("\n")
            );
        }

        if !created_files.is_empty() {
            tracing::debug!(
                "New files created: \n{}",
                created_files.iter().map(|f| format!("- {f}")).join("\n")
            );
        }

        Ok(SyncMetrics {
            database_ref: database.name.to_string(),
            sync_time_secs: start_time.elapsed().as_secs_f64(),
            output_files,
            deleted_files,
            overwritten_files,
            created_files,
            would_overwrite_files,
            dimensions,
        })
    }
    pub async fn sync_all(
        &self,
        filter: crate::service::sync::SyncFilter,
    ) -> Result<Vec<Result<SyncMetrics, OxyError>>, OxyError> {
        let (db_filter, schema_tables) = filter.into_filter();
        let databases = match db_filter {
            Some(db) => {
                tracing::debug!(
                    "Filtering to database: {} with schema_tables: {:?}",
                    db,
                    schema_tables
                );
                let resolved_db = self.config.resolve_database(&db)?;
                tracing::debug!(
                    "Resolved database: {} (type: {:?})",
                    resolved_db.name,
                    resolved_db.database_type
                );
                if schema_tables.is_empty() {
                    vec![resolved_db.clone()]
                } else {
                    vec![resolved_db.clone().with_schema_tables(schema_tables)]
                }
            }
            None => {
                let all_dbs = self.config.list_databases();
                tracing::debug!(
                    "No filter provided, syncing all {} databases",
                    all_dbs.len()
                );
                for db in &all_dbs {
                    tracing::trace!("  - Database: {} (type: {:?})", db.name, db.database_type);
                }
                all_dbs
            }
        };

        tracing::debug!("About to sync {} database(s)", databases.len());
        for db in &databases {
            tracing::trace!(
                "Database to sync: {} (type: {:?})",
                db.name,
                db.database_type
            );
        }
        let pbar = Arc::new(Mutex::new(pbar(Some(databases.len()))));
        let metrics = async_stream::stream! {
          for database in databases {
            let db = database.clone();
            tracing::trace!("Starting sync for database: {}", db.name);
            let pbar = pbar.clone();
            yield async move {
              let result = self.sync(&db, Some(pbar)).await;
              tracing::trace!("Sync completed for database: {} - result: {:?}", db.name, result.is_ok());
              result
            };
          }
        }
        .buffered(10)
        .collect::<Vec<_>>()
        .await;

        Ok(metrics)
    }
}
