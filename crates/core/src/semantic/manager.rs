use itertools::Itertools;
use std::{
    collections::HashSet,
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Instant,
};

use futures::StreamExt;
use tqdm::{Pbar, pbar};

use crate::{
    config::{
        ConfigManager,
        model::{Database, SemanticDimension, SemanticModels},
    },
    errors::OxyError,
    semantic::{loader::SchemaLoader, types::SemanticKey},
};

use super::{
    SemanticContexts, SemanticVariablesContexts,
    contexts::SemanticDimensionsContexts,
    storage::SemanticStorage,
    types::{DatabaseInfo, SemanticTableRef, SyncDimension, SyncMetrics},
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

    async fn load_global_semantics(
        &self,
    ) -> Result<Vec<Result<(String, SemanticModels), OxyError>>, OxyError> {
        let semantics = self.storage.load_global_semantics().await?;
        let models = async_stream::stream! {
            for target in semantics.list_targets() {
                yield async move {
                    let semantic_table_ref = SemanticTableRef::from_str(&target)
                        .map_err(|err| {
                            tracing::error!("Failed to parse semantic table reference {}: {:?}", target, err);
                            err
                        })?;

                    self.storage.load_model(&semantic_table_ref).await.map_err(|err| {
                        tracing::error!("Failed to load model for entity {}: {:?}", target, err);
                        err
                    }).map(|model| {
                        (
                            semantic_table_ref.table,
                            model
                        )
                    })
                };
            }
        }
        .buffered(10)
        .collect::<Vec<_>>()
        .await;
        Ok(models)
    }

    pub async fn get_semantic_contexts(&self) -> Result<SemanticContexts, OxyError> {
        let models = self.load_global_semantics().await?;
        Ok(SemanticContexts::new(
            models.into_iter().filter_map(Result::ok).collect(),
        ))
    }

    pub async fn get_semantic_variables_contexts(
        &self,
    ) -> Result<SemanticVariablesContexts, OxyError> {
        let models = self.load_global_semantics().await?;
        SemanticVariablesContexts::new(models.into_iter().filter_map(Result::ok).collect())
    }

    pub async fn get_semantic_dimensions_contexts(
        &self,
        variables_contexts: &SemanticVariablesContexts,
    ) -> Result<SemanticDimensionsContexts, OxyError> {
        let semantics = self.storage.load_global_semantics().await?;
        Ok(SemanticDimensionsContexts::new(
            semantics
                .dimensions
                .into_iter()
                .map(|dim| (dim.name.clone(), dim))
                .collect(),
            variables_contexts,
        ))
    }

    async fn sync(
        &self,
        database: &Database,
        pbar: Option<Arc<Mutex<Pbar>>>,
    ) -> Result<SyncMetrics, OxyError> {
        let start_time = Instant::now();
        let loader = SchemaLoader::from_database(database, &self.config).await?;
        let semantics = loader.load_schema().await?;
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
            dimensions,
        })
    }
    pub async fn sync_all(
        &self,
        filter: Option<(String, Vec<String>)>,
    ) -> Result<Vec<Result<SyncMetrics, OxyError>>, OxyError> {
        let mut global_semantics = self.storage.load_global_semantics().await?;
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
        let metrics = async_stream::stream! {
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
        .await;

        for result in metrics.iter() {
            if let Ok(metrics) = result {
                for model in &metrics.dimensions {
                    match model {
                        SyncDimension::Created { dimensions, src } => {
                            let dimension_names = dimensions
                                .iter()
                                .map(|d| d.name.clone())
                                .collect::<HashSet<_>>();
                            // Remove targets that no longer exist
                            global_semantics.dimensions.iter_mut().for_each(|dim| {
                                for target in dim.targets.clone().iter() {
                                    if let Some(target_dim_name) =
                                        target.strip_prefix(&src.table_ref())
                                    {
                                        if !dimension_names.contains(target_dim_name) {
                                            dim.targets.retain(|t| t != target);
                                        }
                                    }
                                }
                            });
                            // Add new dimensions
                            for dim in dimension_names {
                                // Find if the dimension already exists and update it or create a new one
                                match global_semantics
                                    .dimensions
                                    .iter_mut()
                                    .find(|d| d.name == dim)
                                {
                                    Some(existing_dim) => {
                                        existing_dim.targets.push(src.to_target(&dim));
                                    }
                                    None => {
                                        global_semantics.dimensions.push(SemanticDimension {
                                            name: dim.clone(),
                                            targets: vec![src.to_target(&dim)],
                                            ..Default::default()
                                        });
                                    }
                                }
                            }
                        }
                        SyncDimension::DeletedRef { src } => {
                            global_semantics.dimensions.iter_mut().for_each(|dim| {
                                dim.targets
                                    .retain(|target| !target.starts_with(&src.table_ref()));
                            })
                        }
                    }
                }
            }
        }

        // Save the updated global semantics
        global_semantics
            .dimensions
            .retain(|dim| !dim.targets.is_empty());
        self.storage
            .save_global_semantics(&global_semantics)
            .await?;
        Ok(metrics)
    }
}
