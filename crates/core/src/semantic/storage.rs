use std::{collections::HashMap, fs::File, path::PathBuf};

use futures::StreamExt;
use tokio::fs::{create_dir_all, read_dir, read_to_string};

use crate::{
    config::{
        ConfigManager,
        constants::{DATABASE_SEMANTIC_PATH, GLOBAL_SEMANTIC_PATH, SEMANTIC_MODEL_PATH},
        model::{SemanticModels, Semantics},
    },
    errors::OxyError,
    utils::get_file_stem,
};

use super::types::{
    DatasetInfo, SemanticKey, SemanticTableRef, SyncDimension, SyncOperationResult,
};

#[enum_dispatch::enum_dispatch]
trait Storage {
    async fn load_global_semantics(&self) -> Result<Semantics, OxyError>;
    async fn save_global_semantics(&self, semantics: &Semantics) -> Result<(), OxyError>;
    async fn load_datasets(&self, database_ref: &str) -> Result<Vec<DatasetInfo>, OxyError>;
    async fn load_ddl(&self, key: &SemanticKey) -> Result<String, OxyError>;
    async fn save_ddl(
        &self,
        key: &SemanticKey,
        value: String,
    ) -> Result<SyncOperationResult, OxyError>;
    async fn load_model(&self, table_ref: &SemanticTableRef) -> Result<SemanticModels, OxyError>;
    async fn load_raw_models(&self, key: &SemanticKey)
    -> Result<HashMap<String, String>, OxyError>; // Loading plaintext to allow easy iterate through the model
    async fn save_model(
        &self,
        key: &SemanticKey,
        value: HashMap<String, SemanticModels>,
    ) -> Result<SyncOperationResult, OxyError>;
}

pub struct SemanticFileStorage {
    global_semantic_path: String,
    base_path: String,
    semantic_path: String,
    override_mode: bool,
}

impl SemanticFileStorage {
    pub async fn from_config(
        config: &ConfigManager,
        override_mode: bool,
    ) -> Result<Self, OxyError> {
        let base_path = config.resolve_file(DATABASE_SEMANTIC_PATH).await?;
        let global_semantic_path = config.resolve_file(GLOBAL_SEMANTIC_PATH).await?;
        Ok(SemanticFileStorage {
            global_semantic_path,
            base_path,
            semantic_path: SEMANTIC_MODEL_PATH.to_string(),
            override_mode,
        })
    }

    fn get_database_dir(&self, database_ref: &str) -> PathBuf {
        PathBuf::from(&self.base_path).join(database_ref)
    }

    fn get_dataset_ddl_path(&self, key: &SemanticKey) -> PathBuf {
        self.get_database_dir(&key.database)
            .join(format!("{}.sql", key.dataset))
    }

    fn get_dataset_semantic_dir(&self, key: &SemanticKey) -> PathBuf {
        self.get_database_dir(&key.database)
            .join(&key.dataset)
            .join(&self.semantic_path)
    }

    fn get_semantic_file_path(&self, key: &SemanticKey, model: &str) -> PathBuf {
        self.get_dataset_semantic_dir(key)
            .join(format!("{model}.sem.yml"))
    }
}

impl Storage for SemanticFileStorage {
    async fn load_global_semantics(&self) -> Result<Semantics, OxyError> {
        let global_semantic_path = PathBuf::from(&self.global_semantic_path);
        if !global_semantic_path.exists() {
            serde_yaml::to_writer(File::create(&global_semantic_path)?, &Semantics::default())
                .map_err(|err| format!("Failed to create default semantics file: {err}"))?;
        }
        let semantics = serde_yaml::from_str::<Semantics>(
            &read_to_string(&global_semantic_path).await.map_err(|err| {
                OxyError::IOError(format!("Failed to load global semantics: {err}"))
            })?,
        )
        .map_err(|err| {
            OxyError::SerializerError(format!("Failed to deserialize global semantics: {err}"))
        })?;
        Ok(semantics)
    }

    async fn save_global_semantics(&self, semantics: &Semantics) -> Result<(), OxyError> {
        tracing::debug!("Saving global semantics to: {}", self.global_semantic_path);
        tracing::trace!("Semantics: {semantics:?}");
        let global_semantic_path = PathBuf::from(&self.global_semantic_path);
        create_dir_all(global_semantic_path.parent().ok_or(OxyError::IOError(
            "Failed to resolve global semantic path".to_string(),
        ))?)
        .await
        .map_err(|err| {
            OxyError::IOError(format!("Failed to create global semantic path: {err}"))
        })?;

        serde_yaml::to_writer(File::create(&global_semantic_path)?, semantics).map_err(|err| {
            OxyError::SerializerError(format!("Failed to save global semantics: {err}"))
        })?;
        Ok(())
    }

    async fn load_model(&self, table_ref: &SemanticTableRef) -> Result<SemanticModels, OxyError> {
        let path = self.get_semantic_file_path(
            &SemanticKey {
                database: table_ref.database.clone(),
                dataset: table_ref.dataset.clone(),
            },
            &table_ref.table,
        );
        let content = read_to_string(path).await.map_err(|err| {
            OxyError::IOError(format!("Failed to read semantic entity file: {err}"))
        })?;
        serde_yaml::from_str(&content).map_err(|err| {
            OxyError::SerializerError(format!("Failed to deserialize semantic entity: {err}"))
        })
    }

    async fn load_datasets(&self, database_ref: &str) -> Result<Vec<DatasetInfo>, OxyError> {
        let db_dir = self.get_database_dir(database_ref);
        if !db_dir.exists() {
            return Err(OxyError::IOError(format!(
                "Failed to load file: {}",
                db_dir.display()
            )));
        }
        let mut read_dir = read_dir(db_dir)
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to list database path: {err}")))?;

        let mut results = vec![];
        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to read database directory: {err}")))?
        {
            let file_type = entry.file_type().await.map_err(|err| {
                OxyError::IOError(format!("Failed to read database file type: {err}"))
            })?;
            if file_type.is_dir() {
                let dataset = entry.file_name().to_string_lossy().to_string();
                let key = SemanticKey::new(database_ref.to_string(), dataset.clone());
                let ddl = self.load_ddl(&key).await.ok();
                let semantic_info = self.load_raw_models(&key).await?;
                results.push(DatasetInfo {
                    dataset,
                    ddl,
                    semantic_info,
                });
            }
        }
        Ok(results)
    }

    async fn load_ddl(&self, key: &SemanticKey) -> Result<String, OxyError> {
        // Implement file loading logic here
        let ddl_path = self.get_dataset_ddl_path(key);
        if !ddl_path.exists() {
            return Err(OxyError::IOError(format!(
                "Failed to load file: {}",
                ddl_path.display()
            )));
        }
        let ddl = read_to_string(ddl_path)
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to load ddl file: {err}")))?;
        Ok(ddl)
    }

    async fn save_ddl(
        &self,
        key: &SemanticKey,
        value: String,
    ) -> Result<SyncOperationResult, OxyError> {
        // Implement file saving logic here
        let ddl_path = self.get_dataset_ddl_path(key);
        let output = ddl_path.to_string_lossy().to_string();
        let existed = ddl_path.exists();

        let mut result = SyncOperationResult::new(output.clone());

        if existed && !self.override_mode {
            result.would_overwrite_files.push(output);
            return Ok(result);
        }

        create_dir_all(ddl_path.parent().ok_or(OxyError::IOError(
            "Failed to resolve database semantic path".to_string(),
        ))?)
        .await
        .map_err(|err| OxyError::IOError(format!("Failed to create ddl parent path: {err}")))?;

        if existed && self.override_mode {
            tokio::fs::remove_file(&ddl_path).await.map_err(|err| {
                OxyError::IOError(format!("Failed to delete existing ddl file: {err}"))
            })?;
            result.overwritten_files.push(output.clone());
        } else {
            result.created_files.push(output.clone());
        }

        tokio::fs::write(ddl_path, value)
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to write ddl file: {err}")))?;

        Ok(result)
    }

    async fn load_raw_models(
        &self,
        key: &SemanticKey,
    ) -> Result<HashMap<String, String>, OxyError> {
        // Implement file loading logic here
        let semantic_path = self.get_dataset_semantic_dir(key);
        if !semantic_path.exists() {
            return Err(OxyError::IOError(format!(
                "Failed to load file: {}",
                semantic_path.display()
            )));
        }
        let mut read_dir = read_dir(semantic_path)
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to list semantic path: {err}")))?;
        let mut results = HashMap::new();
        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to read semantic directory: {err}")))?
        {
            let file_type = entry.file_type().await.map_err(|err| {
                OxyError::IOError(format!("Failed to read semantic file type: {err}"))
            })?;
            if file_type.is_file()
                && entry
                    .file_name()
                    .to_string_lossy()
                    .to_string()
                    .ends_with(".sem.yml")
            {
                let semantic_file_path = entry.path();
                let key = semantic_file_path
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .split(".")
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let semantic_model = read_to_string(semantic_file_path).await.map_err(|err| {
                    OxyError::IOError(format!("Failed to load semantic file: {err}"))
                })?;
                results.insert(key, semantic_model);
            }
        }

        Ok(results)
    }

    async fn save_model(
        &self,
        key: &SemanticKey,
        value: HashMap<String, SemanticModels>,
    ) -> Result<SyncOperationResult, OxyError> {
        // Implement file saving logic here
        let semantic_path = self.get_dataset_semantic_dir(key);
        let output = semantic_path.to_string_lossy().to_string();
        let mut potential_deleted_files = Vec::new();
        let mut created_files = Vec::new();
        let mut overwritten_files = Vec::new();
        let mut would_overwrite_files = Vec::new();

        if semantic_path.exists() {
            if let Ok(mut read_dir) = read_dir(&semantic_path).await {
                while let Ok(Some(entry)) = read_dir.next_entry().await {
                    if let Ok(file_type) = entry.file_type().await {
                        if file_type.is_file() {
                            potential_deleted_files
                                .push(entry.path().to_string_lossy().to_string());
                        }
                    }
                }
            }

            if self.override_mode {
                tokio::fs::remove_dir_all(&semantic_path)
                    .await
                    .map_err(|err| {
                        OxyError::IOError(format!(
                            "Failed to delete existing semantic directory: {err}"
                        ))
                    })?;
            }
        }

        if self.override_mode || !semantic_path.exists() {
            create_dir_all(&semantic_path).await.map_err(|err| {
                OxyError::IOError(format!("Failed to create semantic directory: {err}"))
            })?;
        }

        let potential_deleted_for_filter = potential_deleted_files.clone();

        let model_results = async_stream::stream! {
            for (table_name, model) in value {
                let file_path = self.get_semantic_file_path(key, &table_name);
                let file_path_str = file_path.to_string_lossy().to_string();
                let override_mode = self.override_mode;
                let potential_deleted = potential_deleted_files.clone();
                let mut sync_dimension = None;

                yield async move {
                  let content = serde_yaml::to_string(&model).map_err(|err| {
                    OxyError::IOError(format!("Failed to serialize semantic model: {err}"))
                  })?;

                  let existed = potential_deleted.iter().any(|path| path == &file_path_str);

                  if !override_mode && existed {
                      return Ok::<_, OxyError>((sync_dimension, file_path_str, false, false, true));
                  }

                  if override_mode || !existed {
                        sync_dimension = Some(SyncDimension::Created {
                            dimensions: model.dimensions.clone(),
                            src: SemanticTableRef {
                                database: key.database.clone(),
                                dataset: key.dataset.clone(),
                                table: table_name.clone(),
                            },
                        });
                      tokio::fs::write(file_path, content).await.map_err(|err| {
                          OxyError::IOError(format!("Failed to write semantic file: {err}"))
                      })?;
                  }

                  let created = !existed;
                  let overwritten = existed && override_mode;
                  let would_overwrite = existed && !override_mode;

                  Ok((sync_dimension, file_path_str, created, overwritten, would_overwrite))
                };
            }
        }
        .buffered(10)
        .collect::<Vec<_>>()
        .await;

        let mut dimensions = vec![];
        for result in model_results {
            match result {
                Ok((sync_dimension, file_path, created, overwritten, would_overwrite)) => {
                    if let Some(dim) = sync_dimension {
                        dimensions.push(dim);
                    }

                    if created {
                        created_files.push(file_path.clone());
                    }

                    if overwritten {
                        overwritten_files.push(file_path.clone());
                    }

                    if would_overwrite {
                        would_overwrite_files.push(file_path.clone());
                    }
                }
                Err(e) => return Err(e),
            }
        }

        let deleted_files: Vec<String> = if self.override_mode {
            potential_deleted_for_filter
                .into_iter()
                .filter(|path| !created_files.contains(path) && !overwritten_files.contains(path))
                .collect()
        } else {
            Vec::new()
        };
        for file in &deleted_files {
            if let Some(name) = get_file_stem(file).split('.').next() {
                dimensions.push(SyncDimension::DeletedRef {
                    src: SemanticTableRef {
                        database: key.database.clone(),
                        dataset: key.dataset.clone(),
                        table: name.to_string(),
                    },
                });
            }
        }

        Ok(SyncOperationResult::with_tracking(
            output,
            deleted_files,
            overwritten_files,
            created_files,
            would_overwrite_files,
            dimensions,
        ))
    }
}

#[enum_dispatch::enum_dispatch(Storage)]
pub enum StorageImpl {
    File(SemanticFileStorage),
}

pub struct SemanticStorage {
    storage: StorageImpl,
}

impl SemanticStorage {
    pub async fn from_config(
        config: &ConfigManager,
        override_mode: bool,
    ) -> Result<Self, OxyError> {
        let storage = SemanticFileStorage::from_config(config, override_mode).await?;
        Ok(SemanticStorage {
            storage: StorageImpl::File(storage),
        })
    }

    pub async fn load_global_semantics(&self) -> Result<Semantics, OxyError> {
        self.storage.load_global_semantics().await
    }

    pub async fn save_global_semantics(&self, semantics: &Semantics) -> Result<(), OxyError> {
        self.storage.save_global_semantics(semantics).await
    }

    pub async fn load_datasets(&self, database_ref: &str) -> Result<Vec<DatasetInfo>, OxyError> {
        self.storage.load_datasets(database_ref).await
    }

    pub async fn save_ddl(
        &self,
        key: &SemanticKey,
        value: String,
    ) -> Result<SyncOperationResult, OxyError> {
        self.storage.save_ddl(key, value).await
    }

    pub async fn load_model(
        &self,
        table_ref: &SemanticTableRef,
    ) -> Result<SemanticModels, OxyError> {
        self.storage.load_model(table_ref).await
    }

    pub async fn save_model(
        &self,
        key: &SemanticKey,
        value: HashMap<String, SemanticModels>,
    ) -> Result<SyncOperationResult, OxyError> {
        self.storage.save_model(key, value).await
    }
}
