use uuid::Uuid;

use crate::{adapters::secrets::SecretsStorage, errors::OxyError};
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct SecretsEnvironmentStorage;

impl SecretsEnvironmentStorage {
    /// Get the path to the .env file
    fn get_env_file_path() -> Result<PathBuf, OxyError> {
        let current_dir = env::current_dir().map_err(|e| {
            OxyError::SecretManager(format!("Failed to get current directory: {}", e))
        })?;
        Ok(current_dir.join(".env"))
    }

    /// Read all lines from .env file
    fn read_env_file(path: &PathBuf) -> Result<Vec<String>, OxyError> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(path)
            .map_err(|e| OxyError::SecretManager(format!("Failed to read .env file: {}", e)))?;

        Ok(content.lines().map(|s| s.to_string()).collect())
    }

    /// Write lines back to .env file
    fn write_env_file(path: &PathBuf, lines: Vec<String>) -> Result<(), OxyError> {
        let content = lines.join("\n");
        fs::write(path, content)
            .map_err(|e| OxyError::SecretManager(format!("Failed to write .env file: {}", e)))?;
        Ok(())
    }

    /// Parse a line to extract key-value pair
    fn parse_env_line(line: &str) -> Option<(String, String)> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }

        if let Some(eq_pos) = trimmed.find('=') {
            let key = trimmed[..eq_pos].trim().to_string();
            let value = trimmed[eq_pos + 1..].trim().to_string();
            Some((key, value))
        } else {
            None
        }
    }
}

impl SecretsStorage for SecretsEnvironmentStorage {
    async fn resolve_secret(&self, secret_name: &str) -> Result<Option<String>, OxyError> {
        Ok(std::env::var(secret_name).ok())
    }

    async fn create_secret(
        &self,
        secret_name: &str,
        secret_value: &str,
        _created_by: Uuid,
    ) -> Result<(), OxyError> {
        let env_path = Self::get_env_file_path()?;
        let mut lines = Self::read_env_file(&env_path)?;

        // Check if the secret already exists and update it, or add new
        let mut found = false;
        for line in lines.iter_mut() {
            if let Some((key, _)) = Self::parse_env_line(line)
                && key == secret_name
            {
                *line = format!("{}={}", secret_name, secret_value);
                found = true;
                break;
            }
        }

        if !found {
            lines.push(format!("{}={}", secret_name, secret_value));
        }

        // Write back to .env file
        Self::write_env_file(&env_path, lines)?;

        // Set in current environment
        unsafe {
            env::set_var(secret_name, secret_value);
        }

        Ok(())
    }

    async fn remove_secret(&self, secret_name: &str) -> Result<(), OxyError> {
        let env_path = Self::get_env_file_path()?;
        let lines = Self::read_env_file(&env_path)?;

        // Filter out the line with the matching key
        let filtered_lines: Vec<String> = lines
            .into_iter()
            .filter(|line| {
                if let Some((key, _)) = Self::parse_env_line(line) {
                    key != secret_name
                } else {
                    true // Keep comments and empty lines
                }
            })
            .collect();

        // Write back to .env file
        Self::write_env_file(&env_path, filtered_lines)?;

        // Remove from current environment
        unsafe {
            env::remove_var(secret_name);
        }

        Ok(())
    }
}
