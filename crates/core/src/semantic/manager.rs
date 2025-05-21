use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

use futures::StreamExt;
use tqdm::{Pbar, pbar};

use crate::{
    config::{ConfigManager, model::Database},
    errors::OxyError,
    semantic::{loader::SchemaLoader, types::SemanticKey},
};

use super::{
    storage::SemanticStorage,
    types::{DatabaseInfo, SyncMetrics},
};

pub struct SemanticManager {
    config: ConfigManager,
    storage: SemanticStorage,
}

impl SemanticManager {
    pub async fn from_config(config: ConfigManager, override_mode: bool) -> Result<Self, OxyError> {
        let storage = SemanticStorage::from_config(&config, override_mode).await?;
        Ok(SemanticManager { config, storage })
    }

    pub async fn load_database_info(&self, database_ref: &str) -> Result<DatabaseInfo, OxyError> {
        let database = self.config.resolve_database(database_ref)?;
        let datasets = match self.storage.load_datasets(database_ref).await {
            Ok(datasets) => datasets,
            Err(err) => match err {
                OxyError::IOError(_) => {
                    self.sync(database, None).await?;
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

    async fn sync(
        &self,
        database: &Database,
        pbar: Option<Arc<Mutex<Pbar>>>,
    ) -> Result<SyncMetrics, OxyError> {
        use itertools::Itertools;

        let start_time = Instant::now();
        let loader = SchemaLoader::from_database(database, &self.config).await?;
        let semantics = loader.load_schema().await?;
        let mut output_files = vec![];
        let mut would_overwrite_files: Vec<String> = vec![];
        let mut overwritten_files = vec![];
        let mut deleted_files = vec![];
        let mut created_files = vec![];

        for (dataset, semantic_models) in semantics {
            let key = SemanticKey::new(database.name.to_string(), dataset);
            let result = self.storage.save_model(&key, semantic_models).await?;
            output_files.push(result.base_path.clone());

            would_overwrite_files.extend(result.would_overwrite_files.clone());
            overwritten_files.extend(result.overwritten_files.clone());
            created_files.extend(result.created_files.clone());
            deleted_files.extend(result.deleted_files.clone());
        }

        let ddls = loader.load_ddl().await?;
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
                    .map(|f| format!("- {}", f))
                    .join("\n")
            );
        }

        if !overwritten_files.is_empty() {
            tracing::warn!(
                "Some existing files were overwritten (--overwrite flag was used): \n{}",
                overwritten_files
                    .iter()
                    .map(|f| format!("- {}", f))
                    .join("\n")
            );
        }

        if !deleted_files.is_empty() {
            tracing::warn!(
                "Some files were deleted (not in output): \n{}",
                deleted_files.iter().map(|f| format!("- {}", f)).join("\n")
            );
        }

        if !created_files.is_empty() {
            tracing::debug!(
                "New files created: \n{}",
                created_files.iter().map(|f| format!("- {}", f)).join("\n")
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
        })
    }
    pub async fn sync_all(
        &self,
        filter: Option<(String, Vec<String>)>,
    ) -> Result<Vec<Result<SyncMetrics, OxyError>>, OxyError> {
        let databases = match filter {
            Some((db, datasets)) => vec![
                self.config
                    .resolve_database(&db)?
                    .clone()
                    .with_datasets(datasets),
            ],
            None => self.config.list_databases()?.to_vec(),
        };
        let pbar = Arc::new(Mutex::new(pbar(Some(databases.len()))));
        Ok(async_stream::stream! {
          for database in databases {
            let db = database.clone();
            let pbar = pbar.clone();
            yield async move {
              self.sync(&db, Some(pbar)).await
            };
          }
        }
        .buffered(10)
        .collect::<Vec<_>>()
        .await)
    }
}
