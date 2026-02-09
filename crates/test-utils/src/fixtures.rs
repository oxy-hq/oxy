//! Test fixture utilities for creating temporary test environments.

use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A test fixture that creates a temporary directory with optional pre-populated files.
pub struct TestFixture {
    temp_dir: TempDir,
}

impl TestFixture {
    /// Creates a new empty test fixture.
    pub fn new() -> std::io::Result<Self> {
        Ok(Self {
            temp_dir: TempDir::new()?,
        })
    }

    /// Creates a new test fixture with a custom prefix.
    pub fn with_prefix(prefix: &str) -> std::io::Result<Self> {
        Ok(Self {
            temp_dir: tempfile::Builder::new().prefix(prefix).tempdir()?,
        })
    }

    /// Returns the path to the temporary directory.
    pub fn path(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Creates a file with the given content in the fixture directory.
    pub fn create_file(&self, name: &str, content: &str) -> std::io::Result<PathBuf> {
        let file_path = self.temp_dir.path().join(name);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file_path, content)?;
        Ok(file_path)
    }

    /// Creates a YAML file in the fixture directory.
    pub fn create_yaml<T: serde::Serialize>(
        &self,
        name: &str,
        content: &T,
    ) -> std::io::Result<PathBuf> {
        let yaml_content =
            serde_yaml::to_string(content).map_err(|e| std::io::Error::other(e.to_string()))?;
        self.create_file(name, &yaml_content)
    }

    /// Creates a JSON file in the fixture directory.
    pub fn create_json<T: serde::Serialize>(
        &self,
        name: &str,
        content: &T,
    ) -> std::io::Result<PathBuf> {
        let json_content = serde_json::to_string_pretty(content)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        self.create_file(name, &json_content)
    }

    /// Creates a subdirectory in the fixture.
    pub fn create_dir(&self, name: &str) -> std::io::Result<PathBuf> {
        let dir_path = self.temp_dir.path().join(name);
        std::fs::create_dir_all(&dir_path)?;
        Ok(dir_path)
    }

    /// Keeps the temporary directory (prevents cleanup on drop).
    /// Useful for debugging failed tests.
    pub fn persist(self) -> PathBuf {
        self.temp_dir.into_path()
    }
}

impl Default for TestFixture {
    fn default() -> Self {
        Self::new().expect("Failed to create test fixture")
    }
}

/// Creates a minimal config.yml fixture for testing.
pub fn create_minimal_config(fixture: &TestFixture) -> std::io::Result<PathBuf> {
    let config = r#"
project_name: test-project
defaults:
  database: test_db
"#;
    fixture.create_file("config.yml", config)
}

/// Creates a semantic model fixture for testing.
pub fn create_semantic_model(fixture: &TestFixture, name: &str) -> std::io::Result<PathBuf> {
    let model = format!(
        r#"
name: {name}
description: Test semantic model
entities:
  - name: test_entity
    sql: SELECT * FROM test_table
    dimensions:
      - name: id
        sql: id
        type: number
    measures:
      - name: count
        sql: COUNT(*)
        type: count
"#,
        name = name
    );
    fixture.create_file(&format!("models/{}.yml", name), &model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixture_creates_temp_dir() {
        let fixture = TestFixture::new().unwrap();
        assert!(fixture.path().exists());
    }

    #[test]
    fn test_fixture_creates_file() {
        let fixture = TestFixture::new().unwrap();
        let path = fixture.create_file("test.txt", "hello world").unwrap();
        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello world");
    }

    #[test]
    fn test_fixture_creates_nested_file() {
        let fixture = TestFixture::new().unwrap();
        let path = fixture.create_file("a/b/c/test.txt", "nested").unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_fixture_creates_yaml() {
        let fixture = TestFixture::new().unwrap();
        let data = serde_json::json!({"key": "value"});
        let path = fixture.create_yaml("test.yml", &data).unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("key:"));
    }
}
