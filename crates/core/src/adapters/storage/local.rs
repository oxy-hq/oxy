use std::path::{Path, PathBuf};

use crate::{config::ConfigManager, errors::OxyError};

use super::Storage;

pub struct LocalStorage {
    base_path: String,
}

impl LocalStorage {
    pub async fn from_config<P: AsRef<Path>>(
        config: &ConfigManager,
        base_path: P,
    ) -> Result<Self, OxyError> {
        let base_path = config.resolve_file(base_path).await?;
        Ok(LocalStorage { base_path })
    }

    fn get_path(&self, key: &str) -> impl AsRef<Path> {
        PathBuf::from(&self.base_path).join(key)
    }
}

impl Storage for LocalStorage {
    async fn list(&self, key: &str) -> Result<Vec<String>, OxyError> {
        let path = self.get_path(key);
        let mut entries = tokio::fs::read_dir(path).await?;
        let mut files = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            if let Some(name) = entry.file_name().to_str() {
                files.push(name.to_string());
            }
        }
        Ok(files)
    }

    async fn load(&self, key: &str) -> Result<Vec<u8>, OxyError> {
        let path = self.get_path(key);
        let data = tokio::fs::read(path).await?;
        Ok(data)
    }

    async fn save(&self, key: &str, value: &[u8]) -> Result<String, OxyError> {
        let path = self.get_path(key);
        let output = path.as_ref().to_string_lossy().to_string();
        tokio::fs::write(path, value).await?;
        Ok(output)
    }

    async fn remove(&self, key: &str) -> Result<(), OxyError> {
        let path = self.get_path(key);
        tokio::fs::remove_file(path).await?;
        Ok(())
    }

    async fn glob(&self, pattern: &str) -> Result<Vec<String>, OxyError> {
        // Implement globbing logic here
        unimplemented!()
    }
}
