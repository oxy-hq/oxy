use slugify::slugify;
use sqlformat::{format, FormatOptions, QueryParams};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use crate::errors::OnyxError;
use crate::execute::core::cache::Cache;
use crate::execute::core::value::ContextValue;
use crate::execute::exporter::get_file_directories;

pub struct FileCache;

impl FileCache {
    fn read_str(&self, key: &str) -> Option<String> {
        let path = PathBuf::from(key);
        std::fs::read_to_string(&path).ok()
    }

    fn deserialize(&self, json: &str) -> Option<ContextValue> {
        match serde_json::from_str::<ContextValue>(json) {
            Ok(value) => Some(value),
            Err(e) => {
                log::error!("Error deserializing \n{}\n\n{}", json, e);
                None
            }
        }
    }

    fn write_bytes(&self, key: &str, value: &[u8]) -> Result<(), OnyxError> {
        let path = PathBuf::from(key);
        get_file_directories(&path)?;
        let mut file = File::create(&path).map_err(|e| {
            OnyxError::IOError(format!("Error creating file '{}': {}", path.display(), e))
        })?;
        file.write_all(value).map_err(|e| {
            OnyxError::IOError(format!(
                "Error writing to cache file '{}': {}",
                path.display(),
                e
            ))
        })?;
        Ok(())
    }
}

impl Cache for FileCache {
    fn read(&self, key: &str) -> Option<ContextValue> {
        let json = self.read_str(key)?;
        self.deserialize(&json)
    }

    fn write(&self, key: &str, value: &ContextValue) -> Result<(), OnyxError> {
        let json = serde_json::to_string(value).map_err(|e| {
            OnyxError::SerializerError(format!(
                "Error serializing cache value for key '{}': {}",
                key, e
            ))
        })?;
        self.write_bytes(key, json.as_bytes())
    }
}

pub struct AgentCache {
    file_cache: FileCache,
    key_prefix: String,
}

impl AgentCache {
    pub fn new(key_prefix: String) -> Self {
        Self {
            file_cache: FileCache,
            key_prefix,
        }
    }

    fn compute_cache_key(&self, key: &str) -> Option<String> {
        let path = PathBuf::from(key);
        let dir = path.parent()?;
        let file_name = path.file_name()?.to_string_lossy();
        Some(format!(
            "{}/{}_{}",
            dir.to_string_lossy(),
            slugify!(&self.key_prefix, separator = "_"),
            slugify!(&file_name, separator = "_")
        ))
    }
}

impl Cache for AgentCache {
    fn read(&self, key: &str) -> Option<ContextValue> {
        let maybe_sql = self.file_cache.read_str(key)?;
        if !key.ends_with(".sql") {
            return self.file_cache.deserialize(&maybe_sql);
        }

        if let Some(cache_key) = self.compute_cache_key(key) {
            match self.file_cache.read(&cache_key) {
                Some(ContextValue::Agent(mut agent_output)) => {
                    agent_output.output = Box::new(ContextValue::Text(maybe_sql));
                    Some(ContextValue::Agent(agent_output))
                }
                _ => Some(ContextValue::Text(maybe_sql)),
            }
        } else {
            Some(ContextValue::Text(maybe_sql))
        }
    }

    fn write(&self, key: &str, value: &ContextValue) -> Result<(), OnyxError> {
        if !key.ends_with(".sql") {
            return self.file_cache.write(key, value);
        }

        match value {
            ContextValue::Agent(agent_output) => {
                let formatted_sql = format_sql(&agent_output.output.to_string());
                self.file_cache.write_bytes(key, formatted_sql.as_bytes())?;
                if let Some(cache_key) = self.compute_cache_key(key) {
                    self.file_cache.write(&cache_key, value)?;
                }
                Ok(())
            }
            _ => self.file_cache.write(key, value),
        }
    }
}

fn format_sql(sql: &str) -> String {
    format(sql, &QueryParams::None, &FormatOptions::default())
}
