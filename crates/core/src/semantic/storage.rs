use std::{collections::HashMap, path::PathBuf};

use futures::StreamExt;
use itertools::Itertools;
use tokio::fs::{create_dir_all, read_dir, read_to_string};

use crate::{
    config::{
        ConfigManager,
        constants::{DATABASE_SEMANTIC_PATH, SEMANTIC_MODEL_PATH},
        model::SemanticModels,
    },
    errors::OxyError,
};

use super::types::{DatasetInfo, SemanticKey};

#[enum_dispatch::enum_dispatch]
trait Storage {
    async fn load_datasets(&self, database_ref: &str) -> Result<Vec<DatasetInfo>, OxyError>;
    async fn load_ddl(&self, key: &SemanticKey) -> Result<String, OxyError>;
    async fn save_ddl(&self, key: &SemanticKey, value: String) -> Result<String, OxyError>;
    async fn load_model(&self, key: &SemanticKey) -> Result<HashMap<String, String>, OxyError>; // Loading plaintext to allow easy iterate through the model
    async fn save_model(
        &self,
        key: &SemanticKey,
        value: HashMap<String, SemanticModels>,
    ) -> Result<String, OxyError>;
}

pub struct SemanticFileStorage {
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
        Ok(SemanticFileStorage {
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
            .join(format!("{}.sem.yml", model))
    }
}

impl Storage for SemanticFileStorage {
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
            .map_err(|err| OxyError::IOError(format!("Failed to list database path: {}", err)))?;

        let mut results = vec![];
        while let Some(entry) = read_dir.next_entry().await.map_err(|err| {
            OxyError::IOError(format!("Failed to read database directory: {}", err))
        })? {
            let file_type = entry.file_type().await.map_err(|err| {
                OxyError::IOError(format!("Failed to read database file type: {}", err))
            })?;
            if file_type.is_dir() {
                let dataset = entry.file_name().to_string_lossy().to_string();
                let key = SemanticKey::new(database_ref.to_string(), dataset.clone());
                let ddl = self.load_ddl(&key).await.ok();
                let semantic_info = self.load_model(&key).await?;
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
            .map_err(|err| OxyError::IOError(format!("Failed to load ddl file: {}", err)))?;
        Ok(ddl)
    }

    async fn save_ddl(&self, key: &SemanticKey, value: String) -> Result<String, OxyError> {
        // Implement file saving logic here
        let ddl_path = self.get_dataset_ddl_path(key);
        let output = ddl_path.to_string_lossy().to_string();
        if ddl_path.exists() && !self.override_mode {
            return Ok(output);
        }
        create_dir_all(ddl_path.parent().ok_or(OxyError::IOError(
            "Failed to resolve database semantic path".to_string(),
        ))?)
        .await
        .map_err(|err| OxyError::IOError(format!("Failed to create ddl parent path: {}", err)))?;
        tokio::fs::write(ddl_path, value)
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to write ddl file: {}", err)))?;
        Ok(output)
    }

    async fn load_model(&self, key: &SemanticKey) -> Result<HashMap<String, String>, OxyError> {
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
            .map_err(|err| OxyError::IOError(format!("Failed to list semantic path: {}", err)))?;
        let mut results = HashMap::new();
        while let Some(entry) = read_dir.next_entry().await.map_err(|err| {
            OxyError::IOError(format!("Failed to read semantic directory: {}", err))
        })? {
            let file_type = entry.file_type().await.map_err(|err| {
                OxyError::IOError(format!("Failed to read semantic file type: {}", err))
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
                    .unwrap() // is a file so unwrap is safe here
                    .to_string_lossy()
                    .split(".")
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let semantic_model = read_to_string(semantic_file_path).await.map_err(|err| {
                    OxyError::IOError(format!("Failed to load semantic file: {}", err))
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
    ) -> Result<String, OxyError> {
        // Implement file saving logic here
        let semantic_path = self.get_dataset_semantic_dir(key);
        let output = semantic_path.to_string_lossy().to_string();
        create_dir_all(semantic_path).await.map_err(|err| {
            OxyError::IOError(format!("Failed to create semantic directory: {}", err))
        })?;
        async_stream::stream! {
            for (table_name, model) in value {
                let file_path = self.get_semantic_file_path(key, &table_name);
                let override_mode = self.override_mode;

                yield async move {
                  let content = serde_yaml::to_string(&model).map_err(|err| {
                    OxyError::IOError(format!("Failed to serialize semantic model: {}", err))
                  })?;
                if file_path.exists() && !override_mode {
                    return Result::<(), OxyError>::Ok(());
                }
                  tokio::fs::write(file_path, content).await.map_err(|err| {
                      OxyError::IOError(format!("Failed to write semantic file: {}", err))
                  })?;
                  Result::<(), OxyError>::Ok(())
                };
            }
        }
        .buffered(10)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .try_collect::<(), Vec<_>, _>()?;
        Ok(output)
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

    pub async fn load_datasets(&self, database_ref: &str) -> Result<Vec<DatasetInfo>, OxyError> {
        self.storage.load_datasets(database_ref).await
    }
    pub async fn save_ddl(&self, key: &SemanticKey, value: String) -> Result<String, OxyError> {
        self.storage.save_ddl(key, value).await
    }
    pub async fn save_model(
        &self,
        key: &SemanticKey,
        value: HashMap<String, SemanticModels>,
    ) -> Result<String, OxyError> {
        self.storage.save_model(key, value).await
    }
}
