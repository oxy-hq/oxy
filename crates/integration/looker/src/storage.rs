//! File persistence for Looker metadata
//!
//! This module handles saving and loading Looker explore metadata to/from the filesystem.
//! It manages two parallel directory structures:
//! - Base metadata: `state_dir/.looker/<integration>/<model>/<explore>.yml`
//! - Overlay metadata: `project/looker/<integration>/<model>/<explore>.yml`

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::LookerError;
use crate::metadata::MetadataMerger;
use crate::models::{ExploreMetadata, OverlayExploreMetadata};

/// Storage for Looker metadata files.
///
/// Manages persistence of explore metadata to the filesystem with support for
/// base metadata (synced from Looker API) and overlay metadata (user customizations).
#[derive(Debug, Clone)]
pub struct MetadataStorage {
    /// State directory for base metadata (e.g., ~/.local/share/oxy)
    state_dir: PathBuf,
    /// Project directory for overlay metadata
    project_dir: PathBuf,
    /// Integration name for organizing metadata
    integration_name: String,
}

impl MetadataStorage {
    /// Creates a new MetadataStorage instance.
    ///
    /// # Arguments
    ///
    /// * `state_dir` - Directory for base metadata storage (state_dir/.looker/)
    /// * `project_dir` - Directory for overlay metadata (project/looker/)
    /// * `integration_name` - Name of the Looker integration
    pub fn new<P1: AsRef<Path>, P2: AsRef<Path>>(
        state_dir: P1,
        project_dir: P2,
        integration_name: String,
    ) -> Self {
        Self {
            state_dir: state_dir.as_ref().to_path_buf(),
            project_dir: project_dir.as_ref().to_path_buf(),
            integration_name,
        }
    }

    /// Returns the base metadata directory path for a model.
    ///
    /// Format: `state_dir/.looker/<integration>/<model>/`
    pub fn get_base_metadata_dir(&self, model: &str) -> PathBuf {
        self.state_dir
            .join(".looker")
            .join(&self.integration_name)
            .join(model)
    }

    /// Returns the overlay metadata directory path for a model.
    ///
    /// Format: `project/looker/<integration>/<model>/`
    pub fn get_overlay_metadata_dir(&self, model: &str) -> PathBuf {
        self.project_dir
            .join("looker")
            .join(&self.integration_name)
            .join(model)
    }

    /// Returns the base metadata file path for an explore.
    fn get_base_metadata_path(&self, model: &str, explore: &str) -> PathBuf {
        self.get_base_metadata_dir(model)
            .join(format!("{}.yml", explore))
    }

    /// Returns the overlay metadata file path for an explore.
    fn get_overlay_metadata_path(&self, model: &str, explore: &str) -> PathBuf {
        self.get_overlay_metadata_dir(model)
            .join(format!("{}.yml", explore))
    }

    /// Ensures the directory structure exists for both base and overlay metadata.
    pub fn ensure_directory_structure(&self, model: &str) -> Result<(), LookerError> {
        let base_dir = self.get_base_metadata_dir(model);
        let overlay_dir = self.get_overlay_metadata_dir(model);

        fs::create_dir_all(&base_dir).map_err(|e| LookerError::SyncError {
            message: format!(
                "Failed to create base metadata directory {:?}: {}",
                base_dir, e
            ),
        })?;

        fs::create_dir_all(&overlay_dir).map_err(|e| LookerError::SyncError {
            message: format!(
                "Failed to create overlay metadata directory {:?}: {}",
                overlay_dir, e
            ),
        })?;

        Ok(())
    }

    /// Saves base metadata for an explore.
    ///
    /// The metadata is saved to `state_dir/.looker/<integration>/<model>/<explore>.yml`.
    pub fn save_base_metadata(
        &self,
        model: &str,
        explore: &str,
        metadata: &ExploreMetadata,
    ) -> Result<(), LookerError> {
        self.ensure_directory_structure(model)?;

        let path = self.get_base_metadata_path(model, explore);
        let yaml = serde_yaml::to_string(metadata).map_err(|e| LookerError::SyncError {
            message: format!("Failed to serialize metadata: {}", e),
        })?;

        fs::write(&path, yaml).map_err(|e| LookerError::SyncError {
            message: format!("Failed to write metadata to {:?}: {}", path, e),
        })?;

        Ok(())
    }

    /// Saves overlay metadata for an explore.
    ///
    /// The metadata is saved to `project/looker/<integration>/<model>/<explore>.yml`.
    pub fn save_overlay_metadata(
        &self,
        model: &str,
        explore: &str,
        metadata: &OverlayExploreMetadata,
    ) -> Result<(), LookerError> {
        self.ensure_directory_structure(model)?;

        let path = self.get_overlay_metadata_path(model, explore);
        let yaml = serde_yaml::to_string(metadata).map_err(|e| LookerError::SyncError {
            message: format!("Failed to serialize overlay metadata: {}", e),
        })?;

        fs::write(&path, yaml).map_err(|e| LookerError::SyncError {
            message: format!("Failed to write overlay metadata to {:?}: {}", path, e),
        })?;

        Ok(())
    }

    /// Loads base metadata for an explore.
    pub fn load_base_metadata(
        &self,
        model: &str,
        explore: &str,
    ) -> Result<ExploreMetadata, LookerError> {
        let path = self.get_base_metadata_path(model, explore);

        let content = fs::read_to_string(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                LookerError::NotFoundError {
                    resource: format!("metadata for {}/{}", model, explore),
                }
            } else {
                LookerError::SyncError {
                    message: format!("Failed to read metadata from {:?}: {}", path, e),
                }
            }
        })?;

        serde_yaml::from_str(&content).map_err(|e| LookerError::SyncError {
            message: format!("Failed to parse metadata from {:?}: {}", path, e),
        })
    }

    /// Loads overlay metadata for an explore.
    ///
    /// Returns `Ok(None)` if no overlay metadata exists.
    pub fn load_overlay_metadata(
        &self,
        model: &str,
        explore: &str,
    ) -> Result<Option<OverlayExploreMetadata>, LookerError> {
        let path = self.get_overlay_metadata_path(model, explore);

        match fs::read_to_string(&path) {
            Ok(content) => {
                let metadata =
                    serde_yaml::from_str(&content).map_err(|e| LookerError::SyncError {
                        message: format!("Failed to parse overlay metadata from {:?}: {}", path, e),
                    })?;
                Ok(Some(metadata))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(LookerError::SyncError {
                message: format!("Failed to read overlay metadata from {:?}: {}", path, e),
            }),
        }
    }

    /// Loads and merges base and overlay metadata for an explore.
    ///
    /// Returns the base metadata merged with any overlay customizations.
    /// If no overlay exists, returns the base metadata unchanged.
    pub fn load_merged_metadata(
        &self,
        model: &str,
        explore: &str,
    ) -> Result<ExploreMetadata, LookerError> {
        let base = self.load_base_metadata(model, explore)?;
        let overlay = self.load_overlay_metadata(model, explore)?;

        Ok(MetadataMerger::merge(base, overlay))
    }

    /// Checks if base metadata exists for an explore.
    pub fn base_metadata_exists(&self, model: &str, explore: &str) -> bool {
        self.get_base_metadata_path(model, explore).exists()
    }

    /// Checks if overlay metadata exists for an explore.
    pub fn overlay_metadata_exists(&self, model: &str, explore: &str) -> bool {
        self.get_overlay_metadata_path(model, explore).exists()
    }

    /// Lists all explores with base metadata for a model.
    pub fn list_base_explores(&self, model: &str) -> Result<Vec<String>, LookerError> {
        self.list_explores_in_directory(&self.get_base_metadata_dir(model))
    }

    /// Lists all explores with overlay metadata for a model.
    pub fn list_overlay_explores(&self, model: &str) -> Result<Vec<String>, LookerError> {
        self.list_explores_in_directory(&self.get_overlay_metadata_dir(model))
    }

    /// Lists explore names from YAML files in a directory.
    fn list_explores_in_directory(&self, dir: &Path) -> Result<Vec<String>, LookerError> {
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(dir).map_err(|e| LookerError::SyncError {
            message: format!("Failed to read directory {:?}: {}", dir, e),
        })?;

        let mut explores = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| LookerError::SyncError {
                message: format!("Failed to read directory entry: {}", e),
            })?;

            let path = entry.path();
            if path
                .extension()
                .map_or(false, |ext| ext == "yml" || ext == "yaml")
            {
                if let Some(stem) = path.file_stem() {
                    if let Some(name) = stem.to_str() {
                        explores.push(name.to_string());
                    }
                }
            }
        }

        explores.sort();
        Ok(explores)
    }

    /// Deletes base metadata for an explore.
    pub fn delete_base_metadata(&self, model: &str, explore: &str) -> Result<(), LookerError> {
        let path = self.get_base_metadata_path(model, explore);
        if path.exists() {
            fs::remove_file(&path).map_err(|e| LookerError::SyncError {
                message: format!("Failed to delete metadata file {:?}: {}", path, e),
            })?;
        }
        Ok(())
    }

    /// Deletes overlay metadata for an explore.
    pub fn delete_overlay_metadata(&self, model: &str, explore: &str) -> Result<(), LookerError> {
        let path = self.get_overlay_metadata_path(model, explore);
        if path.exists() {
            fs::remove_file(&path).map_err(|e| LookerError::SyncError {
                message: format!("Failed to delete overlay metadata file {:?}: {}", path, e),
            })?;
        }
        Ok(())
    }

    /// Returns the integration name.
    pub fn integration_name(&self) -> &str {
        &self.integration_name
    }

    /// Returns the state directory.
    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    /// Returns the project directory.
    pub fn project_dir(&self) -> &Path {
        &self.project_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{FieldMetadata, ViewMetadata};
    use tempfile::TempDir;

    fn create_test_storage() -> (MetadataStorage, TempDir, TempDir) {
        let state_dir = TempDir::new().unwrap();
        let project_dir = TempDir::new().unwrap();
        let storage = MetadataStorage::new(
            state_dir.path(),
            project_dir.path(),
            "test_integration".to_string(),
        );
        (storage, state_dir, project_dir)
    }

    fn create_test_metadata() -> ExploreMetadata {
        ExploreMetadata {
            model: "ecommerce".to_string(),
            name: "orders".to_string(),
            base_view_name: Some("orders".to_string()),
            label: Some("Orders".to_string()),
            description: Some("Order analytics".to_string()),
            views: vec![ViewMetadata {
                name: "orders".to_string(),
                dimensions: vec![FieldMetadata {
                    name: "id".to_string(),
                    label: Some("ID".to_string()),
                    description: None,
                    field_type: "dimension".to_string(),
                    data_type: Some("number".to_string()),
                    sql: None,
                    agent_hint: None,
                    examples: None,
                }],
                measures: vec![FieldMetadata {
                    name: "count".to_string(),
                    label: Some("Count".to_string()),
                    description: None,
                    field_type: "measure".to_string(),
                    data_type: Some("number".to_string()),
                    sql: None,
                    agent_hint: None,
                    examples: None,
                }],
            }],
        }
    }

    #[test]
    fn test_directory_paths() {
        let (storage, _state_dir, _project_dir) = create_test_storage();

        let base_dir = storage.get_base_metadata_dir("ecommerce");
        assert!(base_dir.ends_with(".looker/test_integration/ecommerce"));

        let overlay_dir = storage.get_overlay_metadata_dir("ecommerce");
        assert!(overlay_dir.ends_with("looker/test_integration/ecommerce"));
    }

    #[test]
    fn test_save_and_load_base_metadata() {
        let (storage, _state_dir, _project_dir) = create_test_storage();
        let metadata = create_test_metadata();

        // Save metadata
        storage
            .save_base_metadata("ecommerce", "orders", &metadata)
            .unwrap();

        // Verify file exists
        assert!(storage.base_metadata_exists("ecommerce", "orders"));

        // Load and verify
        let loaded = storage.load_base_metadata("ecommerce", "orders").unwrap();
        assert_eq!(loaded.name, "orders");
        assert_eq!(loaded.model, "ecommerce");
        assert_eq!(loaded.views.len(), 1);
    }

    #[test]
    fn test_save_and_load_overlay_metadata() {
        let (storage, _state_dir, _project_dir) = create_test_storage();

        let overlay = OverlayExploreMetadata {
            description: Some("Custom description".to_string()),
            views: None,
        };

        // Save overlay
        storage
            .save_overlay_metadata("ecommerce", "orders", &overlay)
            .unwrap();

        // Verify file exists
        assert!(storage.overlay_metadata_exists("ecommerce", "orders"));

        // Load and verify
        let loaded = storage
            .load_overlay_metadata("ecommerce", "orders")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.description, Some("Custom description".to_string()));
    }

    #[test]
    fn test_load_nonexistent_base_metadata() {
        let (storage, _state_dir, _project_dir) = create_test_storage();

        let result = storage.load_base_metadata("nonexistent", "explore");
        assert!(matches!(result, Err(LookerError::NotFoundError { .. })));
    }

    #[test]
    fn test_load_nonexistent_overlay_metadata() {
        let (storage, _state_dir, _project_dir) = create_test_storage();

        let result = storage
            .load_overlay_metadata("nonexistent", "explore")
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_base_explores() {
        let (storage, _state_dir, _project_dir) = create_test_storage();

        // Save multiple explores
        let metadata1 = create_test_metadata();
        let mut metadata2 = create_test_metadata();
        metadata2.name = "users".to_string();

        storage
            .save_base_metadata("ecommerce", "orders", &metadata1)
            .unwrap();
        storage
            .save_base_metadata("ecommerce", "users", &metadata2)
            .unwrap();

        // List explores
        let explores = storage.list_base_explores("ecommerce").unwrap();
        assert_eq!(explores.len(), 2);
        assert!(explores.contains(&"orders".to_string()));
        assert!(explores.contains(&"users".to_string()));
    }

    #[test]
    fn test_list_explores_empty_model() {
        let (storage, _state_dir, _project_dir) = create_test_storage();

        let explores = storage.list_base_explores("nonexistent").unwrap();
        assert!(explores.is_empty());
    }

    #[test]
    fn test_delete_base_metadata() {
        let (storage, _state_dir, _project_dir) = create_test_storage();
        let metadata = create_test_metadata();

        // Save and verify
        storage
            .save_base_metadata("ecommerce", "orders", &metadata)
            .unwrap();
        assert!(storage.base_metadata_exists("ecommerce", "orders"));

        // Delete and verify
        storage.delete_base_metadata("ecommerce", "orders").unwrap();
        assert!(!storage.base_metadata_exists("ecommerce", "orders"));
    }

    #[test]
    fn test_delete_nonexistent_metadata() {
        let (storage, _state_dir, _project_dir) = create_test_storage();

        // Should not error when deleting nonexistent file
        storage
            .delete_base_metadata("nonexistent", "explore")
            .unwrap();
    }

    #[test]
    fn test_load_merged_metadata_no_overlay() {
        let (storage, _state_dir, _project_dir) = create_test_storage();
        let metadata = create_test_metadata();

        // Save base metadata only
        storage
            .save_base_metadata("ecommerce", "orders", &metadata)
            .unwrap();

        // Load merged (should return base unchanged)
        let merged = storage.load_merged_metadata("ecommerce", "orders").unwrap();
        assert_eq!(merged.name, "orders");
        assert_eq!(merged.model, "ecommerce");
        assert_eq!(merged.description, Some("Order analytics".to_string()));
    }

    #[test]
    fn test_load_merged_metadata_with_overlay() {
        use crate::models::OverlayFieldMetadata;
        use crate::models::OverlayViewMetadata;

        let (storage, _state_dir, _project_dir) = create_test_storage();
        let metadata = create_test_metadata();

        // Save base metadata
        storage
            .save_base_metadata("ecommerce", "orders", &metadata)
            .unwrap();

        // Save overlay metadata with custom description and agent hint
        let overlay = OverlayExploreMetadata {
            description: Some("Custom overlay description".to_string()),
            views: Some(vec![OverlayViewMetadata {
                name: "orders".to_string(),
                dimensions: Some(vec![OverlayFieldMetadata {
                    name: "id".to_string(),
                    description: None,
                    agent_hint: Some("Use this for order lookups".to_string()),
                    examples: None,
                }]),
                measures: None,
            }]),
        };
        storage
            .save_overlay_metadata("ecommerce", "orders", &overlay)
            .unwrap();

        // Load merged
        let merged = storage.load_merged_metadata("ecommerce", "orders").unwrap();

        // Check overlay description is applied
        assert_eq!(
            merged.description,
            Some("Custom overlay description".to_string())
        );

        // Check agent hint is applied to field
        let orders_view = merged.views.iter().find(|v| v.name == "orders").unwrap();
        let id_field = orders_view
            .dimensions
            .iter()
            .find(|d| d.name == "id")
            .unwrap();
        assert_eq!(
            id_field.agent_hint,
            Some("Use this for order lookups".to_string())
        );
    }
}
