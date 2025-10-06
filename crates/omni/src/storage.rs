use std::fs;
use std::path::{Path, PathBuf};

use crate::error::OmniError;
use crate::models::TopicMetadata;

/// Manages metadata storage for Omni integration
///
/// Handles directory structure creation and YAML file operations for both
/// base Omni metadata (.omni folder) and custom overlay metadata (omni folder)
#[derive(Debug, Clone)]
pub struct MetadataStorage {
    project_path: PathBuf,
    integration_name: String,
}

impl MetadataStorage {
    /// Create a new MetadataStorage instance
    pub fn new<P: AsRef<Path>>(project_path: P, integration_name: String) -> Self {
        Self {
            project_path: project_path.as_ref().to_path_buf(),
            integration_name,
        }
    }

    /// Ensure the directory structure exists for storing metadata
    ///
    /// Creates both .omni and omni directories with integration_name/model_id subdirectories
    pub fn ensure_directory_structure(&self, model_id: &str) -> Result<(), OmniError> {
        let base_dir = self.get_base_metadata_dir(model_id);
        let overlay_dir = self.get_overlay_metadata_dir(model_id);

        // Create .omni/<integration_name>/<model_id> directory
        if let Err(e) = fs::create_dir_all(&base_dir) {
            return Err(OmniError::StorageError(format!(
                "Failed to create base metadata directory '{}': {}",
                base_dir.display(),
                e
            )));
        }

        // Create omni/<integration_name>/<model_id> directory
        if let Err(e) = fs::create_dir_all(&overlay_dir) {
            return Err(OmniError::StorageError(format!(
                "Failed to create overlay metadata directory '{}': {}",
                overlay_dir.display(),
                e
            )));
        }

        Ok(())
    }

    /// Save topic metadata to the base metadata directory (.omni)
    pub fn save_base_metadata(
        &self,
        model_id: &str,
        topic_metadata: &TopicMetadata,
    ) -> Result<(), OmniError> {
        let file_path = self.get_base_metadata_file_path(model_id, &topic_metadata.name);
        self.save_metadata_to_file(&file_path, topic_metadata)
    }

    /// Save topic metadata to the overlay metadata directory (omni)
    pub fn save_overlay_metadata(
        &self,
        model_id: &str,
        topic_metadata: &TopicMetadata,
    ) -> Result<(), OmniError> {
        let file_path = self.get_overlay_metadata_file_path(model_id, &topic_metadata.name);
        self.save_metadata_to_file(&file_path, topic_metadata)
    }

    /// Save overlay topic metadata directly to the overlay metadata directory (omni)
    /// This method accepts OverlayTopicMetadata which has optional fields for user customization
    pub fn save_overlay_metadata_direct(
        &self,
        model_id: &str,
        overlay_metadata: &crate::models::OverlayTopicMetadata,
    ) -> Result<(), OmniError> {
        let file_path = self.get_overlay_metadata_file_path(model_id, &overlay_metadata.name);
        self.save_overlay_metadata_to_file(&file_path, overlay_metadata)
    }

    /// Load base topic metadata from .omni directory
    pub fn load_base_metadata(
        &self,
        model_id: &str,
        topic_name: &str,
    ) -> Result<Option<TopicMetadata>, OmniError> {
        let file_path = self.get_base_metadata_file_path(model_id, topic_name);
        self.load_metadata_from_file(&file_path)
    }

    /// Load overlay topic metadata from omni directory
    /// Tries to load as OverlayTopicMetadata first, then falls back to regular TopicMetadata
    /// for backward compatibility, and converts to TopicMetadata for merging
    pub fn load_overlay_metadata(
        &self,
        model_id: &str,
        topic_name: &str,
    ) -> Result<Option<TopicMetadata>, OmniError> {
        let file_path = self.get_overlay_metadata_file_path(model_id, topic_name);

        if !file_path.exists() {
            return Ok(None);
        }

        // First try to load as OverlayTopicMetadata (new format)
        match self.load_overlay_metadata_from_file(&file_path) {
            Ok(Some(overlay_metadata)) => {
                // Convert overlay metadata to regular metadata for merging
                Ok(Some(overlay_metadata.into()))
            }
            Ok(None) => Ok(None),
            Err(_) => {
                // Fall back to loading as regular TopicMetadata (old format)
                // This maintains backward compatibility
                self.load_metadata_from_file(&file_path)
            }
        }
    }

    /// Load overlay topic metadata directly as OverlayTopicMetadata from omni directory
    /// This preserves the optional structure for further editing
    pub fn load_overlay_metadata_direct(
        &self,
        model_id: &str,
        topic_name: &str,
    ) -> Result<Option<crate::models::OverlayTopicMetadata>, OmniError> {
        let file_path = self.get_overlay_metadata_file_path(model_id, topic_name);
        self.load_overlay_metadata_from_file(&file_path)
    }

    /// Load merged topic metadata by combining base and overlay metadata
    ///
    /// This method loads both base (.omni) and overlay (omni) metadata and performs
    /// a sophisticated deep merge where overlay metadata takes precedence. The merge
    /// operates at a granular level:
    ///
    /// - **Topic fields**: Overlay values override base values
    /// - **Views**: Merged by name, with dimensions and measures merged individually
    /// - **Dimensions**: Merged by (field_name, view_name) key, overlay takes precedence
    /// - **Measures**: Merged by (field_name, view_name) key, overlay takes precedence
    /// - **Custom fields**: Overlay values are used when present
    ///
    /// If only one source exists, that metadata is returned.
    /// If neither exists, None is returned.
    ///
    /// # Arguments
    ///
    /// * `model_id` - The model identifier to load metadata for
    /// * `topic_name` - The topic name to load metadata for
    ///
    /// # Returns
    ///
    /// * `Ok(Some(TopicMetadata))` - Successfully loaded and merged metadata
    /// * `Ok(None)` - No metadata found for the given model_id and topic_name
    /// * `Err(OmniError)` - Error occurred while loading or merging metadata
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni::MetadataStorage;
    ///
    /// let storage = MetadataStorage::new("/path/to/project", "my_integration".to_string());
    ///
    /// // Load merged metadata combining base and overlay sources with deep merge
    /// match storage.load_merged_metadata("model_123", "sales_data") {
    ///     Ok(Some(metadata)) => {
    ///         println!("Found merged metadata for topic: {}", metadata.name);
    ///         // The metadata includes deep-merged views, dimensions, and measures
    ///         for view in &metadata.views {
    ///             println!("View '{}' has {} dimensions and {} measures",
    ///                 view.name, view.dimensions.len(), view.measures.len());
    ///         }
    ///         // Custom overlay fields are preserved
    ///         if let Some(description) = metadata.custom_description {
    ///             println!("Custom description: {}", description);
    ///         }
    ///     },
    ///     Ok(None) => println!("No metadata found for this topic"),
    ///     Err(e) => println!("Error loading metadata: {}", e),
    /// }
    /// ```
    pub fn load_merged_metadata(
        &self,
        model_id: &str,
        topic_name: &str,
    ) -> Result<Option<TopicMetadata>, OmniError> {
        let base_metadata = self.load_base_metadata(model_id, topic_name)?;
        let overlay_metadata = self.load_overlay_metadata(model_id, topic_name)?;

        match (base_metadata, overlay_metadata) {
            // If both exist, merge them with overlay taking precedence
            (Some(base), Some(overlay)) => Ok(Some(self.merge_topic_metadata(base, overlay)?)),
            // If only overlay exists, return it
            (None, Some(overlay)) => Ok(Some(overlay)),
            // If only base exists, return it
            (Some(base), None) => Ok(Some(base)),
            // If neither exists, return None
            (None, None) => Ok(None),
        }
    }

    /// List all topic names that have base metadata stored
    pub fn list_base_topics(&self, model_id: &str) -> Result<Vec<String>, OmniError> {
        let base_dir = self.get_base_metadata_dir(model_id);
        self.list_topics_in_directory(&base_dir)
    }

    /// List all topic names that have overlay metadata stored
    pub fn list_overlay_topics(&self, model_id: &str) -> Result<Vec<String>, OmniError> {
        let overlay_dir = self.get_overlay_metadata_dir(model_id);
        self.list_topics_in_directory(&overlay_dir)
    }

    /// Check if base metadata exists for a topic
    pub fn base_metadata_exists(&self, model_id: &str, topic_name: &str) -> bool {
        let file_path = self.get_base_metadata_file_path(model_id, topic_name);
        file_path.exists()
    }

    /// Check if overlay metadata exists for a topic
    pub fn overlay_metadata_exists(&self, model_id: &str, topic_name: &str) -> bool {
        let file_path = self.get_overlay_metadata_file_path(model_id, topic_name);
        file_path.exists()
    }

    /// Delete base metadata for a topic
    pub fn delete_base_metadata(&self, model_id: &str, topic_name: &str) -> Result<(), OmniError> {
        let file_path = self.get_base_metadata_file_path(model_id, topic_name);
        self.delete_metadata_file(&file_path)
    }

    /// Delete overlay metadata for a topic
    pub fn delete_overlay_metadata(
        &self,
        model_id: &str,
        topic_name: &str,
    ) -> Result<(), OmniError> {
        let file_path = self.get_overlay_metadata_file_path(model_id, topic_name);
        self.delete_metadata_file(&file_path)
    }

    /// Get the base metadata directory path (.omni/<integration_name>/<model_id>)
    fn get_base_metadata_dir(&self, model_id: &str) -> PathBuf {
        self.project_path
            .join(".omni")
            .join(&self.integration_name)
            .join(model_id)
    }

    /// Get the overlay metadata directory path (omni/<integration_name>/<model_id>)
    fn get_overlay_metadata_dir(&self, model_id: &str) -> PathBuf {
        self.project_path
            .join("omni")
            .join(&self.integration_name)
            .join(model_id)
    }

    /// Get the base metadata file path (.omni/<integration_name>/<model_id>/<topic_name>.yml)
    fn get_base_metadata_file_path(&self, model_id: &str, topic_name: &str) -> PathBuf {
        self.get_base_metadata_dir(model_id)
            .join(format!("{}.yml", topic_name))
    }

    /// Get the overlay metadata file path (omni/<integration_name>/<model_id>/<topic_name>.yml)
    fn get_overlay_metadata_file_path(&self, model_id: &str, topic_name: &str) -> PathBuf {
        self.get_overlay_metadata_dir(model_id)
            .join(format!("{}.yml", topic_name))
    }

    /// Save metadata to a YAML file
    fn save_metadata_to_file(
        &self,
        file_path: &Path,
        topic_metadata: &TopicMetadata,
    ) -> Result<(), OmniError> {
        // Ensure parent directory exists
        if let Some(parent) = file_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                return Err(OmniError::StorageError(format!(
                    "Failed to create directory '{}': {}",
                    parent.display(),
                    e
                )));
            }
        }

        // Serialize to YAML
        let yaml_content = serde_yaml::to_string(topic_metadata).map_err(|e| {
            OmniError::StorageError(format!("Failed to serialize metadata to YAML: {}", e))
        })?;

        // Write to file
        if let Err(e) = fs::write(file_path, yaml_content) {
            return Err(OmniError::StorageError(format!(
                "Failed to write metadata file '{}': {}",
                file_path.display(),
                e
            )));
        }

        Ok(())
    }

    /// Save overlay metadata to a YAML file
    fn save_overlay_metadata_to_file(
        &self,
        file_path: &Path,
        overlay_metadata: &crate::models::OverlayTopicMetadata,
    ) -> Result<(), OmniError> {
        // Ensure parent directory exists
        if let Some(parent) = file_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                return Err(OmniError::StorageError(format!(
                    "Failed to create directory '{}': {}",
                    parent.display(),
                    e
                )));
            }
        }

        // Serialize to YAML
        let yaml_content = serde_yaml::to_string(overlay_metadata).map_err(|e| {
            OmniError::StorageError(format!(
                "Failed to serialize overlay metadata to YAML: {}",
                e
            ))
        })?;

        // Write to file
        if let Err(e) = fs::write(file_path, yaml_content) {
            return Err(OmniError::StorageError(format!(
                "Failed to write overlay metadata file '{}': {}",
                file_path.display(),
                e
            )));
        }

        Ok(())
    }

    /// Load metadata from a YAML file
    fn load_metadata_from_file(
        &self,
        file_path: &Path,
    ) -> Result<Option<TopicMetadata>, OmniError> {
        if !file_path.exists() {
            return Ok(None);
        }

        // Read file content
        let yaml_content = fs::read_to_string(file_path).map_err(|e| {
            OmniError::StorageError(format!(
                "Failed to read metadata file '{}': {}",
                file_path.display(),
                e
            ))
        })?;

        // Validate that the file is not empty
        if yaml_content.trim().is_empty() {
            return Err(OmniError::StorageError(format!(
                "Metadata file '{}' is empty",
                file_path.display()
            )));
        }

        // Deserialize from YAML
        let topic_metadata: TopicMetadata = serde_yaml::from_str(&yaml_content).map_err(|e| {
            OmniError::StorageError(format!(
                "Failed to deserialize metadata from YAML file '{}': {}",
                file_path.display(),
                e
            ))
        })?;

        // Validate metadata structure
        self.validate_topic_metadata(&topic_metadata)?;

        Ok(Some(topic_metadata))
    }

    /// Load overlay metadata from a YAML file
    fn load_overlay_metadata_from_file(
        &self,
        file_path: &Path,
    ) -> Result<Option<crate::models::OverlayTopicMetadata>, OmniError> {
        if !file_path.exists() {
            return Ok(None);
        }

        // Read file content
        let yaml_content = fs::read_to_string(file_path).map_err(|e| {
            OmniError::StorageError(format!(
                "Failed to read overlay metadata file '{}': {}",
                file_path.display(),
                e
            ))
        })?;

        // Validate that the file is not empty
        if yaml_content.trim().is_empty() {
            return Err(OmniError::StorageError(format!(
                "Overlay metadata file '{}' is empty",
                file_path.display()
            )));
        }

        // Deserialize from YAML
        let overlay_metadata: crate::models::OverlayTopicMetadata =
            serde_yaml::from_str(&yaml_content).map_err(|e| {
                OmniError::StorageError(format!(
                    "Failed to deserialize overlay metadata from YAML file '{}': {}",
                    file_path.display(),
                    e
                ))
            })?;

        // Validate overlay metadata structure
        self.validate_overlay_topic_metadata(&overlay_metadata)?;

        Ok(Some(overlay_metadata))
    }

    /// List topics in a directory by scanning for .yml files
    fn list_topics_in_directory(&self, dir_path: &Path) -> Result<Vec<String>, OmniError> {
        if !dir_path.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(dir_path).map_err(|e| {
            OmniError::StorageError(format!(
                "Failed to read directory '{}': {}",
                dir_path.display(),
                e
            ))
        })?;

        let mut topics = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|e| {
                OmniError::StorageError(format!(
                    "Failed to read directory entry in '{}': {}",
                    dir_path.display(),
                    e
                ))
            })?;

            let path = entry.path();
            if path.is_file() {
                if let Some(extension) = path.extension() {
                    if extension == "yaml" || extension == "yml" {
                        if let Some(stem) = path.file_stem() {
                            if let Some(topic_name) = stem.to_str() {
                                topics.push(topic_name.to_string());
                            }
                        }
                    }
                }
            }
        }

        topics.sort();
        Ok(topics)
    }

    /// Delete a metadata file
    fn delete_metadata_file(&self, file_path: &Path) -> Result<(), OmniError> {
        if !file_path.exists() {
            return Ok(()); // File doesn't exist, nothing to delete
        }

        fs::remove_file(file_path).map_err(|e| {
            OmniError::StorageError(format!(
                "Failed to delete metadata file '{}': {}",
                file_path.display(),
                e
            ))
        })
    }

    /// Merge base and overlay topic metadata with overlay taking precedence
    ///
    /// This method combines metadata from two sources:
    /// - Base metadata (from .omni directory) - typically generated from Omni API
    /// - Overlay metadata (from omni directory) - user customizations
    ///
    /// Uses the sophisticated deep merge logic from MetadataMerger to properly
    /// merge views, dimensions, and measures at a granular level.
    ///
    /// # Arguments
    ///
    /// * `base` - The base topic metadata (usually from Omni API)
    /// * `overlay` - The overlay topic metadata (user customizations)
    ///
    /// # Returns
    ///
    /// * `Ok(TopicMetadata)` - Successfully merged metadata
    /// * `Err(OmniError)` - Error occurred during merging (currently only validation errors)
    fn merge_topic_metadata(
        &self,
        base: TopicMetadata,
        overlay: TopicMetadata,
    ) -> Result<TopicMetadata, OmniError> {
        // Use the sophisticated merge logic from MetadataMerger
        use crate::metadata::MetadataMerger;
        let merged = MetadataMerger::merge_topic_metadata(base, Some(overlay));
        Ok(merged)
    }

    /// Validate topic metadata structure
    fn validate_topic_metadata(&self, metadata: &TopicMetadata) -> Result<(), OmniError> {
        // Check that topic name is not empty
        if metadata.name.trim().is_empty() {
            return Err(OmniError::ValidationError(
                "Topic name cannot be empty".to_string(),
            ));
        }

        // Validate views
        for view in &metadata.views {
            if view.name.trim().is_empty() {
                return Err(OmniError::ValidationError(
                    "View name cannot be empty".to_string(),
                ));
            }

            // Validate dimensions
            for dimension in &view.dimensions {
                if dimension.field_name.trim().is_empty() {
                    return Err(OmniError::ValidationError(
                        "Dimension field name cannot be empty".to_string(),
                    ));
                }
                if dimension.view_name.trim().is_empty() {
                    return Err(OmniError::ValidationError(
                        "Dimension view name cannot be empty".to_string(),
                    ));
                }
                if dimension.data_type.trim().is_empty() {
                    return Err(OmniError::ValidationError(
                        "Dimension data type cannot be empty".to_string(),
                    ));
                }
            }

            // Validate measures
            for measure in &view.measures {
                if measure.field_name.trim().is_empty() {
                    return Err(OmniError::ValidationError(
                        "Measure field name cannot be empty".to_string(),
                    ));
                }
                if measure.view_name.trim().is_empty() {
                    return Err(OmniError::ValidationError(
                        "Measure view name cannot be empty".to_string(),
                    ));
                }
                if measure.data_type.trim().is_empty() {
                    return Err(OmniError::ValidationError(
                        "Measure data type cannot be empty".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    /// Validate overlay topic metadata structure
    /// Only validates required identifier fields since other fields are optional
    fn validate_overlay_topic_metadata(
        &self,
        metadata: &crate::models::OverlayTopicMetadata,
    ) -> Result<(), OmniError> {
        // Check that topic name is not empty
        if metadata.name.trim().is_empty() {
            return Err(OmniError::ValidationError(
                "Topic name cannot be empty".to_string(),
            ));
        }

        // Validate views if present
        if let Some(views) = &metadata.views {
            for view in views {
                if view.name.trim().is_empty() {
                    return Err(OmniError::ValidationError(
                        "View name cannot be empty".to_string(),
                    ));
                }

                // Validate dimensions if present
                if let Some(dimensions) = &view.dimensions {
                    for dimension in dimensions {
                        if dimension.field_name.trim().is_empty() {
                            return Err(OmniError::ValidationError(
                                "Dimension field name cannot be empty".to_string(),
                            ));
                        }
                        if dimension.view_name.trim().is_empty() {
                            return Err(OmniError::ValidationError(
                                "Dimension view name cannot be empty".to_string(),
                            ));
                        }
                        // data_type and fully_qualified_name are optional in overlay
                        if let Some(data_type) = &dimension.data_type {
                            if data_type.trim().is_empty() {
                                return Err(OmniError::ValidationError(
                                    "Dimension data type cannot be empty when specified"
                                        .to_string(),
                                ));
                            }
                        }
                    }
                }

                // Validate measures if present
                if let Some(measures) = &view.measures {
                    for measure in measures {
                        if measure.field_name.trim().is_empty() {
                            return Err(OmniError::ValidationError(
                                "Measure field name cannot be empty".to_string(),
                            ));
                        }
                        if measure.view_name.trim().is_empty() {
                            return Err(OmniError::ValidationError(
                                "Measure view name cannot be empty".to_string(),
                            ));
                        }
                        // data_type and fully_qualified_name are optional in overlay
                        if let Some(data_type) = &measure.data_type {
                            if data_type.trim().is_empty() {
                                return Err(OmniError::ValidationError(
                                    "Measure data type cannot be empty when specified".to_string(),
                                ));
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DimensionMetadata, MeasureMetadata, QueryExample, ViewMetadata};
    use tempfile::TempDir;

    fn create_test_topic_metadata(name: &str) -> TopicMetadata {
        TopicMetadata {
            name: name.to_string(),
            label: Some(format!("{} Label", name)),
            views: vec![ViewMetadata {
                name: "test_view".to_string(),
                dimensions: vec![DimensionMetadata {
                    field_name: "dim1".to_string(),
                    view_name: "test_view".to_string(),
                    data_type: "string".to_string(),
                    fully_qualified_name: "test_view.dim1".to_string(),
                    description: Some("Test dimension".to_string()),
                    ai_context: Some("This is a test dimension for AI analysis".to_string()),
                    label: Some("Dimension 1".to_string()),
                }],
                measures: vec![MeasureMetadata {
                    field_name: "measure1".to_string(),
                    view_name: "test_view".to_string(),
                    data_type: "number".to_string(),
                    fully_qualified_name: "test_view.measure1".to_string(),
                    description: Some("Test measure".to_string()),
                    ai_context: Some("This is a test measure for AI analysis".to_string()),
                    label: Some("Measure 1".to_string()),
                }],
                filter_only_fields: vec!["filter1".to_string()],
            }],
            custom_description: None,
            agent_hints: None,
            examples: None,
        }
    }

    fn create_overlay_topic_metadata(name: &str) -> TopicMetadata {
        TopicMetadata {
            name: name.to_string(),
            label: Some(format!("{} Overlay Label", name)),
            views: vec![ViewMetadata {
                name: "test_view".to_string(),
                dimensions: vec![DimensionMetadata {
                    field_name: "dim1".to_string(),
                    view_name: "test_view".to_string(),
                    data_type: "string".to_string(),
                    fully_qualified_name: "test_view.dim1".to_string(),
                    description: Some("Test dimension overlay".to_string()),
                    ai_context: Some("AI context for overlay dimension".to_string()),
                    label: Some("Overlay Dimension 1".to_string()),
                }],
                measures: vec![MeasureMetadata {
                    field_name: "measure1".to_string(),
                    view_name: "test_view".to_string(),
                    data_type: "number".to_string(),
                    fully_qualified_name: "test_view.measure1".to_string(),
                    description: Some("Test measure overlay".to_string()),
                    ai_context: Some("AI context for overlay measure".to_string()),
                    label: Some("Overlay Measure 1".to_string()),
                }],
                filter_only_fields: vec!["filter1".to_string()],
            }],
            custom_description: Some("Custom overlay description".to_string()),
            agent_hints: Some(vec!["hint1".to_string(), "hint2".to_string()]),
            examples: Some(vec![QueryExample {
                description: "Example Query".to_string(),
                query: "SELECT * FROM test_view".to_string(),
                expected_result: Some("Test result".to_string()),
            }]),
        }
    }

    #[test]
    fn test_load_merged_metadata_with_both_base_and_overlay() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MetadataStorage::new(temp_dir.path(), "test_integration".to_string());
        let model_id = "test_model";
        let topic_name = "test_topic";

        // Create base metadata
        let base_metadata = create_test_topic_metadata(topic_name);
        storage
            .save_base_metadata(model_id, &base_metadata)
            .unwrap();

        // Create overlay metadata with custom fields
        let overlay_metadata = create_overlay_topic_metadata(topic_name);
        storage
            .save_overlay_metadata(model_id, &overlay_metadata)
            .unwrap();

        // Load merged metadata
        let merged = storage.load_merged_metadata(model_id, topic_name).unwrap();
        assert!(merged.is_some());

        let merged_metadata = merged.unwrap();

        // Check that overlay fields take precedence
        assert_eq!(merged_metadata.name, topic_name);
        assert_eq!(
            merged_metadata.label,
            Some(format!("{} Overlay Label", topic_name))
        );
        assert_eq!(
            merged_metadata.custom_description,
            Some("Custom overlay description".to_string())
        );
        assert_eq!(
            merged_metadata.agent_hints,
            Some(vec!["hint1".to_string(), "hint2".to_string()])
        );
        assert!(merged_metadata.examples.is_some());

        // Check that views are properly merged
        assert_eq!(merged_metadata.views.len(), 1);
        assert_eq!(merged_metadata.views[0].name, "test_view");
        assert_eq!(
            merged_metadata.views[0].dimensions[0].description,
            Some("Test dimension overlay".to_string())
        );
    }

    #[test]
    fn test_load_merged_metadata_with_only_base() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MetadataStorage::new(temp_dir.path(), "test_integration".to_string());
        let model_id = "test_model";
        let topic_name = "test_topic";

        // Create only base metadata
        let base_metadata = create_test_topic_metadata(topic_name);
        storage
            .save_base_metadata(model_id, &base_metadata)
            .unwrap();

        // Load merged metadata
        let merged = storage.load_merged_metadata(model_id, topic_name).unwrap();
        assert!(merged.is_some());

        let merged_metadata = merged.unwrap();

        // Should return base metadata as-is
        assert_eq!(merged_metadata.name, topic_name);
        assert_eq!(merged_metadata.label, Some(format!("{} Label", topic_name)));
        assert_eq!(merged_metadata.custom_description, None);
        assert_eq!(merged_metadata.agent_hints, None);
        assert_eq!(merged_metadata.examples, None);
    }

    #[test]
    fn test_load_merged_metadata_with_only_overlay() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MetadataStorage::new(temp_dir.path(), "test_integration".to_string());
        let model_id = "test_model";
        let topic_name = "test_topic";

        // Create only overlay metadata
        let overlay_metadata = create_overlay_topic_metadata(topic_name);
        storage
            .save_overlay_metadata(model_id, &overlay_metadata)
            .unwrap();

        // Load merged metadata
        let merged = storage.load_merged_metadata(model_id, topic_name).unwrap();
        assert!(merged.is_some());

        let merged_metadata = merged.unwrap();

        // Should return overlay metadata as-is
        assert_eq!(merged_metadata.name, topic_name);
        assert_eq!(
            merged_metadata.label,
            Some(format!("{} Overlay Label", topic_name))
        );
        assert_eq!(
            merged_metadata.custom_description,
            Some("Custom overlay description".to_string())
        );
        assert_eq!(
            merged_metadata.agent_hints,
            Some(vec!["hint1".to_string(), "hint2".to_string()])
        );
        assert!(merged_metadata.examples.is_some());
    }

    #[test]
    fn test_load_merged_metadata_with_neither() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MetadataStorage::new(temp_dir.path(), "test_integration".to_string());
        let model_id = "test_model";
        let topic_name = "nonexistent_topic";

        // Load merged metadata for non-existent topic
        let merged = storage.load_merged_metadata(model_id, topic_name).unwrap();
        assert!(merged.is_none());
    }

    #[test]
    fn test_merge_topic_metadata_functionality() {
        let storage = MetadataStorage::new("/tmp", "test_integration".to_string());

        let base = create_test_topic_metadata("base_topic");
        let overlay = create_overlay_topic_metadata("overlay_topic");

        let merged = storage.merge_topic_metadata(base, overlay).unwrap();

        // Check overlay takes precedence for basic fields
        assert_eq!(merged.name, "overlay_topic");
        assert_eq!(
            merged.label,
            Some("overlay_topic Overlay Label".to_string())
        );

        // Check custom overlay fields are present
        assert_eq!(
            merged.custom_description,
            Some("Custom overlay description".to_string())
        );
        assert_eq!(
            merged.agent_hints,
            Some(vec!["hint1".to_string(), "hint2".to_string()])
        );
        assert!(merged.examples.is_some());

        // Check views are properly handled
        assert_eq!(merged.views.len(), 1);
        assert_eq!(merged.views[0].name, "test_view");
    }

    #[test]
    fn test_deep_merge_functionality() {
        let storage = MetadataStorage::new("/tmp", "test_integration".to_string());

        // Create base metadata with multiple dimensions and measures
        let mut base = create_test_topic_metadata("test_topic");
        base.views[0].dimensions.push(DimensionMetadata {
            field_name: "dim2".to_string(),
            view_name: "test_view".to_string(),
            data_type: "integer".to_string(),
            fully_qualified_name: "test_view.dim2".to_string(),
            description: Some("Base dim2 description".to_string()),
            ai_context: Some("Base dimension 2 for AI context".to_string()),
            label: Some("Base Dimension 2".to_string()),
        });
        base.views[0].measures.push(MeasureMetadata {
            field_name: "measure2".to_string(),
            view_name: "test_view".to_string(),
            data_type: "decimal".to_string(),
            fully_qualified_name: "test_view.measure2".to_string(),
            description: Some("Base measure2 description".to_string()),
            ai_context: Some("Base measure 2 for AI context".to_string()),
            label: Some("Base Measure 2".to_string()),
        });

        // Create overlay metadata that modifies some fields and adds new ones
        let overlay = TopicMetadata {
            name: "test_topic".to_string(),
            label: Some("Overlay Label".to_string()),
            views: vec![ViewMetadata {
                name: "test_view".to_string(),
                dimensions: vec![
                    // Override existing dim1 with new description
                    DimensionMetadata {
                        field_name: "dim1".to_string(),
                        view_name: "test_view".to_string(),
                        data_type: "string".to_string(),
                        fully_qualified_name: "test_view.dim1".to_string(),
                        description: Some("OVERLAY dim1 description".to_string()),
                        ai_context: Some("Overlay AI context for dim1".to_string()),
                        label: Some("Overlay Dim1".to_string()),
                    },
                    // Add new dim3
                    DimensionMetadata {
                        field_name: "dim3".to_string(),
                        view_name: "test_view".to_string(),
                        data_type: "boolean".to_string(),
                        fully_qualified_name: "test_view.dim3".to_string(),
                        description: Some("New dim3 from overlay".to_string()),
                        ai_context: Some("New dimension 3 AI context".to_string()),
                        label: Some("New Dim3".to_string()),
                    },
                ],
                measures: vec![
                    // Override existing measure1
                    MeasureMetadata {
                        field_name: "measure1".to_string(),
                        view_name: "test_view".to_string(),
                        data_type: "bigint".to_string(), // Changed from "number"
                        fully_qualified_name: "test_view.measure1".to_string(),
                        description: Some("Overlay measure1 description".to_string()),
                        ai_context: Some("Overlay AI context for measure1".to_string()),
                        label: Some("Overlay Measure1".to_string()),
                    },
                ],
                filter_only_fields: vec!["new_filter".to_string()],
            }],
            custom_description: Some("Deep merge test description".to_string()),
            agent_hints: None,
            examples: None,
        };

        let merged = storage.merge_topic_metadata(base, overlay).unwrap();

        // Check that we have all dimensions: base dim2 + overlay dim1 (modified) + overlay dim3 (new)
        assert_eq!(merged.views[0].dimensions.len(), 3);

        // Check dim1 has overlay description
        let dim1 = merged.views[0]
            .dimensions
            .iter()
            .find(|d| d.field_name == "dim1")
            .unwrap();
        assert_eq!(
            dim1.description,
            Some("OVERLAY dim1 description".to_string())
        );

        // Check dim2 is preserved from base
        let dim2 = merged.views[0]
            .dimensions
            .iter()
            .find(|d| d.field_name == "dim2")
            .unwrap();
        assert_eq!(dim2.description, Some("Base dim2 description".to_string()));

        // Check dim3 is new from overlay
        let dim3 = merged.views[0]
            .dimensions
            .iter()
            .find(|d| d.field_name == "dim3")
            .unwrap();
        assert_eq!(dim3.description, Some("New dim3 from overlay".to_string()));

        // Check measures: base measure2 + overlay measure1 (modified)
        assert_eq!(merged.views[0].measures.len(), 2);

        // Check measure1 has overlay data type
        let measure1 = merged.views[0]
            .measures
            .iter()
            .find(|m| m.field_name == "measure1")
            .unwrap();
        assert_eq!(measure1.data_type, "bigint");

        // Check measure2 is preserved from base
        let measure2 = merged.views[0]
            .measures
            .iter()
            .find(|m| m.field_name == "measure2")
            .unwrap();
        assert_eq!(measure2.data_type, "decimal");

        // Check filter_only_fields is replaced (not merged)
        assert_eq!(
            merged.views[0].filter_only_fields,
            vec!["new_filter".to_string()]
        );
    }

    #[test]
    fn test_overlay_metadata_save_and_load() {
        use crate::models::{
            OverlayDimensionMetadata, OverlayMeasureMetadata, OverlayTopicMetadata,
            OverlayViewMetadata,
        };

        let temp_dir = TempDir::new().unwrap();
        let storage = MetadataStorage::new(temp_dir.path(), "test_integration".to_string());
        let model_id = "test_model";
        let topic_name = "test_topic";

        // Create overlay metadata with only some fields specified
        let overlay_metadata = OverlayTopicMetadata {
            name: topic_name.to_string(),
            label: Some("Custom Topic Label".to_string()),
            views: Some(vec![OverlayViewMetadata {
                name: "test_view".to_string(),
                dimensions: Some(vec![OverlayDimensionMetadata {
                    field_name: "dim1".to_string(),
                    view_name: "test_view".to_string(),
                    data_type: None,            // Not specified - will use default
                    fully_qualified_name: None, // Not specified - will be generated
                    description: Some("Custom dimension description".to_string()),
                    ai_context: Some("Custom AI context for dimension".to_string()),
                    label: Some("Custom Dimension Label".to_string()),
                }]),
                measures: Some(vec![OverlayMeasureMetadata {
                    field_name: "measure1".to_string(),
                    view_name: "test_view".to_string(),
                    data_type: Some("custom_number".to_string()), // Override the data type
                    fully_qualified_name: None, // Not specified - will be generated
                    description: Some("Custom measure description".to_string()),
                    ai_context: Some("Custom AI context for measure".to_string()),
                    label: Some("Custom Measure Label".to_string()),
                }]),
                filter_only_fields: None, // Not specified
            }]),
            custom_description: Some("This is a custom description".to_string()),
            agent_hints: None, // Not specified
            examples: None,    // Not specified
        };

        // Save overlay metadata
        storage
            .save_overlay_metadata_direct(model_id, &overlay_metadata)
            .unwrap();

        // Load back as overlay metadata
        let loaded_overlay = storage
            .load_overlay_metadata_direct(model_id, topic_name)
            .unwrap();
        assert!(loaded_overlay.is_some());

        let loaded = loaded_overlay.unwrap();
        assert_eq!(loaded.name, topic_name);
        assert_eq!(loaded.label, Some("Custom Topic Label".to_string()));
        assert_eq!(
            loaded.custom_description,
            Some("This is a custom description".to_string())
        );
        assert!(loaded.agent_hints.is_none());
        assert!(loaded.examples.is_none());

        // Check views
        assert!(loaded.views.is_some());
        let views = loaded.views.as_ref().unwrap();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].name, "test_view");

        // Check dimensions
        assert!(views[0].dimensions.is_some());
        let dimensions = views[0].dimensions.as_ref().unwrap();
        assert_eq!(dimensions.len(), 1);
        assert_eq!(dimensions[0].field_name, "dim1");
        assert_eq!(dimensions[0].view_name, "test_view");
        assert_eq!(dimensions[0].data_type, None); // Should remain None
        assert_eq!(dimensions[0].fully_qualified_name, None); // Should remain None
        assert_eq!(
            dimensions[0].description,
            Some("Custom dimension description".to_string())
        );

        // Check measures
        assert!(views[0].measures.is_some());
        let measures = views[0].measures.as_ref().unwrap();
        assert_eq!(measures.len(), 1);
        assert_eq!(measures[0].field_name, "measure1");
        assert_eq!(measures[0].view_name, "test_view");
        assert_eq!(measures[0].data_type, Some("custom_number".to_string()));
        assert_eq!(measures[0].fully_qualified_name, None); // Should remain None

        // Check filter_only_fields
        assert!(views[0].filter_only_fields.is_none());
    }

    #[test]
    fn test_overlay_metadata_conversion_to_regular() {
        use crate::models::{
            OverlayDimensionMetadata, OverlayMeasureMetadata, OverlayTopicMetadata,
            OverlayViewMetadata,
        };

        // Create overlay metadata with some fields missing
        let overlay_metadata = OverlayTopicMetadata {
            name: "test_topic".to_string(),
            label: Some("Test Label".to_string()),
            views: Some(vec![OverlayViewMetadata {
                name: "test_view".to_string(),
                dimensions: Some(vec![OverlayDimensionMetadata {
                    field_name: "dim1".to_string(),
                    view_name: "test_view".to_string(),
                    data_type: None, // Missing - should default to "string"
                    fully_qualified_name: None, // Missing - should be generated
                    description: Some("Test dimension".to_string()),
                    ai_context: Some("Test AI context for dimension".to_string()),
                    label: Some("Test Dimension Label".to_string()),
                }]),
                measures: Some(vec![OverlayMeasureMetadata {
                    field_name: "measure1".to_string(),
                    view_name: "test_view".to_string(),
                    data_type: None, // Missing - should default to "number"
                    fully_qualified_name: None, // Missing - should be generated
                    description: Some("Test measure".to_string()),
                    ai_context: Some("Test AI context for measure".to_string()),
                    label: Some("Test Measure Label".to_string()),
                }]),
                filter_only_fields: None, // Missing - should default to empty vec
            }]),
            custom_description: Some("Custom description".to_string()),
            agent_hints: None, // Missing - should remain None
            examples: None,    // Missing - should remain None
        };

        // Convert to regular metadata
        let regular_metadata: TopicMetadata = overlay_metadata.into();

        // Check basic fields
        assert_eq!(regular_metadata.name, "test_topic");
        assert_eq!(regular_metadata.label, Some("Test Label".to_string()));
        assert_eq!(
            regular_metadata.custom_description,
            Some("Custom description".to_string())
        );
        assert_eq!(regular_metadata.agent_hints, None);
        assert_eq!(regular_metadata.examples, None);

        // Check views
        assert_eq!(regular_metadata.views.len(), 1);
        assert_eq!(regular_metadata.views[0].name, "test_view");

        // Check dimensions - defaults should be filled in
        assert_eq!(regular_metadata.views[0].dimensions.len(), 1);
        let dim = &regular_metadata.views[0].dimensions[0];
        assert_eq!(dim.field_name, "dim1");
        assert_eq!(dim.view_name, "test_view");
        assert_eq!(dim.data_type, "string"); // Default
        assert_eq!(dim.fully_qualified_name, "test_view.dim1"); // Generated
        assert_eq!(dim.description, Some("Test dimension".to_string()));

        // Check measures - defaults should be filled in
        assert_eq!(regular_metadata.views[0].measures.len(), 1);
        let measure = &regular_metadata.views[0].measures[0];
        assert_eq!(measure.field_name, "measure1");
        assert_eq!(measure.view_name, "test_view");
        assert_eq!(measure.data_type, "number"); // Default
        assert_eq!(measure.fully_qualified_name, "test_view.measure1"); // Generated

        // Check filter_only_fields - should be empty vec
        assert_eq!(
            regular_metadata.views[0].filter_only_fields,
            Vec::<String>::new()
        );
    }

    #[test]
    fn test_overlay_metadata_backward_compatibility() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MetadataStorage::new(temp_dir.path(), "test_integration".to_string());
        let model_id = "test_model";
        let topic_name = "test_topic";

        // Save as regular metadata (simulating old format)
        let regular_metadata = create_test_topic_metadata(topic_name);
        storage
            .save_overlay_metadata(model_id, &regular_metadata)
            .unwrap();

        // Should be able to load via the overlay method (backward compatibility)
        let loaded = storage.load_overlay_metadata(model_id, topic_name).unwrap();
        assert!(loaded.is_some());

        let loaded_metadata = loaded.unwrap();
        assert_eq!(loaded_metadata.name, topic_name);
        assert_eq!(loaded_metadata.label, Some(format!("{} Label", topic_name)));
    }

    #[test]
    fn test_integration_name_directory_structure() {
        let temp_dir = TempDir::new().unwrap();
        let integration_name = "my_integration";
        let storage = MetadataStorage::new(temp_dir.path(), integration_name.to_string());
        let model_id = "test_model";
        let topic_name = "test_topic";

        // Create and save metadata
        let metadata = create_test_topic_metadata(topic_name);
        storage.save_base_metadata(model_id, &metadata).unwrap();
        storage.save_overlay_metadata(model_id, &metadata).unwrap();

        // Verify directory structure includes integration name
        let base_dir = temp_dir
            .path()
            .join(".omni")
            .join(integration_name)
            .join(model_id);
        let overlay_dir = temp_dir
            .path()
            .join("omni")
            .join(integration_name)
            .join(model_id);

        assert!(
            base_dir.exists(),
            "Base directory should exist: {}",
            base_dir.display()
        );
        assert!(
            overlay_dir.exists(),
            "Overlay directory should exist: {}",
            overlay_dir.display()
        );

        // Verify file paths use .yml extension
        let base_file = base_dir.join(format!("{}.yml", topic_name));
        let overlay_file = overlay_dir.join(format!("{}.yml", topic_name));

        assert!(
            base_file.exists(),
            "Base file should exist: {}",
            base_file.display()
        );
        assert!(
            overlay_file.exists(),
            "Overlay file should exist: {}",
            overlay_file.display()
        );

        // Verify we can load the metadata
        let loaded = storage.load_merged_metadata(model_id, topic_name).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().name, topic_name);
    }
}
