use crate::{
    config::model::Task,
    errors::OxyError,
    execute::{
        ExecutionContext,
        builders::cache::{CacheStorage, CacheWriter, Cacheable},
        types::{Output, OutputContainer},
    },
    utils::get_file_directories,
};
use serde::de::DeserializeOwned;
use slugify::slugify;
use sqlformat::{FormatOptions, QueryParams, format};
use std::{io::Write, path::PathBuf};

use super::task::TaskInput;

#[derive(Clone)]
pub struct TaskCacheable;

#[async_trait::async_trait]
impl Cacheable<Task> for TaskCacheable {
    async fn cache_key(&self, execution_context: &ExecutionContext, task: &Task) -> Option<String> {
        let cache_config = task.cache.clone()?;
        if cache_config.enabled {
            let rendered_path = execution_context.renderer.render(&cache_config.path).ok()?;
            let cache_key = execution_context
                .config
                .resolve_file(&rendered_path)
                .await
                .ok()?;
            Some(cache_key)
        } else {
            None
        }
    }
}

#[derive(Clone)]
pub struct TaskCacheStorage {
    cacheable: TaskCacheable,
}

impl TaskCacheStorage {
    pub fn new() -> Self {
        Self {
            cacheable: TaskCacheable,
        }
    }

    fn compute_cache_key(&self, prefix: &str, key: &str) -> Option<String> {
        let path = PathBuf::from(key);
        let dir = path.parent()?;
        let file_name = path.file_name()?.to_string_lossy();
        Some(format!(
            "{}/{}_{}",
            dir.to_string_lossy(),
            slugify!(prefix, separator = "_"),
            slugify!(&file_name, separator = "_")
        ))
    }
}

#[async_trait::async_trait]
impl CacheStorage<TaskInput, OutputContainer> for TaskCacheStorage {
    async fn read(
        &self,
        execution_context: &ExecutionContext,
        input: &TaskInput,
    ) -> Option<OutputContainer> {
        let file_cache = FileCache {};
        let key = self
            .cacheable
            .cache_key(execution_context, &input.task)
            .await?;
        let maybe_sql = file_cache.read_str(&key)?;

        if let Some(cache_key) = self.compute_cache_key(&input.task.name, &key) {
            log::debug!("Cache key: {}", cache_key);
            let output = file_cache.read::<OutputContainer>(&cache_key).await;
            log::debug!("May be SQL: {}\n{:?}", maybe_sql, output);
            match output {
                Some(OutputContainer::Single(Output::SQL(mut sql))) => {
                    sql.0 = maybe_sql;
                    Some(OutputContainer::Single(sql.into()))
                }
                Some(OutputContainer::Metadata {
                    output: Output::SQL(mut sql),
                    metadata,
                }) => {
                    sql.0 = maybe_sql;
                    Some(OutputContainer::Metadata {
                        output: sql.into(),
                        metadata,
                    })
                }
                _ => file_cache.deserialize(&maybe_sql),
            }
        } else {
            file_cache.deserialize(&maybe_sql)
        }
    }
    async fn write(
        &self,
        execution_context: &ExecutionContext,
        input: &TaskInput,
        value: &OutputContainer,
    ) -> Result<(), OxyError> {
        let file_cache = FileCache {};
        if let Some(key) = self
            .cacheable
            .cache_key(execution_context, &input.task)
            .await
        {
            match value {
                OutputContainer::Single(Output::SQL(sql)) => {
                    let formatted_sql = format_sql(&sql.0.to_string());
                    file_cache.write_bytes(&key, formatted_sql.as_bytes())?;

                    if let Some(cache_key) = self.compute_cache_key(&input.task.name, &key) {
                        file_cache.write(&cache_key, value).await?;
                    }
                }
                OutputContainer::Metadata {
                    output: Output::SQL(sql),
                    metadata: _,
                } => {
                    let formatted_sql = format_sql(&sql.0.to_string());
                    file_cache.write_bytes(&key, formatted_sql.as_bytes())?;

                    if let Some(cache_key) = self.compute_cache_key(&input.task.name, &key) {
                        file_cache.write(&cache_key, value).await?;
                    }
                }
                _ => {
                    file_cache.write(&key, value).await?;
                }
            }
        }
        Ok(())
    }
}

pub struct FileCache;

impl FileCache {
    fn read_str(&self, key: &str) -> Option<String> {
        let path = PathBuf::from(key);
        std::fs::read_to_string(&path).ok()
    }

    fn deserialize<R: DeserializeOwned>(&self, json: &str) -> Option<R> {
        match serde_json::from_str::<R>(json) {
            Ok(value) => Some(value),
            Err(e) => {
                log::error!("Error deserializing \n{}\n\n{}", json, e);
                None
            }
        }
    }

    fn write_bytes(&self, key: &str, value: &[u8]) -> Result<(), OxyError> {
        let path = PathBuf::from(key);
        get_file_directories(&path)?;
        let mut file = std::fs::File::create(&path).map_err(|e| {
            OxyError::IOError(format!("Error creating file '{}': {}", path.display(), e))
        })?;
        file.write_all(value).map_err(|e| {
            OxyError::IOError(format!(
                "Error writing to cache file '{}': {}",
                path.display(),
                e
            ))
        })?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl CacheWriter for FileCache {
    async fn read<R: serde::de::DeserializeOwned>(&self, key: &str) -> Option<R> {
        let json = self.read_str(key)?;
        self.deserialize(&json)
    }

    async fn write<R: serde::Serialize + Sync>(
        &self,
        key: &str,
        value: &R,
    ) -> Result<(), OxyError> {
        let json = serde_json::to_string(value).map_err(|e| {
            OxyError::SerializerError(format!(
                "Error serializing cache value for key '{}': {}",
                key, e
            ))
        })?;
        self.write_bytes(key, json.as_bytes())
    }
}

fn format_sql(sql: &str) -> String {
    format(sql, &QueryParams::None, &FormatOptions::default())
}
