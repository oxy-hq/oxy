use std::path::PathBuf;

use omni::{MetadataMerger, MetadataStorage, OmniApiClient, OmniError, TopicMetadata};

/// Result of a full model synchronization operation
///
/// Contains statistics about the sync operation including successful,
/// failed, and skipped topics with detailed error information.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncResult {
    /// The model ID that was synchronized
    pub model_id: String,
    /// Total number of topics found in the model
    pub total_topics: usize,
    /// List of topics that were successfully synchronized
    pub successful_topics: Vec<String>,
    /// List of topics that failed to synchronize with error details
    pub failed_topics: Vec<TopicSyncError>,
    /// List of topics that were skipped (for future use)
    pub skipped_topics: Vec<String>,
}

/// Error information for a failed topic synchronization
#[derive(Debug, Clone, PartialEq)]
pub struct TopicSyncError {
    /// Name of the topic that failed to sync
    pub topic_name: String,
    /// Error message describing what went wrong
    pub error: String,
}

impl SyncResult {
    /// Check if the sync operation was completely successful
    pub fn is_success(&self) -> bool {
        self.failed_topics.is_empty()
    }

    /// Check if the sync operation was partially successful
    pub fn is_partial_success(&self) -> bool {
        !self.successful_topics.is_empty() && !self.failed_topics.is_empty()
    }

    /// Check if the sync operation completely failed
    pub fn is_failure(&self) -> bool {
        self.successful_topics.is_empty() && !self.failed_topics.is_empty()
    }

    /// Get the success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        if self.total_topics == 0 {
            return 100.0;
        }
        (self.successful_topics.len() as f64 / self.total_topics as f64) * 100.0
    }

    /// Get a summary string of the sync operation
    pub fn summary(&self) -> String {
        format!(
            "Synchronized {}/{} topics ({:.1}% success rate)",
            self.successful_topics.len(),
            self.total_topics,
            self.success_rate()
        )
    }

    /// Get detailed error information for failed topics
    pub fn error_summary(&self) -> Option<String> {
        if self.failed_topics.is_empty() {
            return None;
        }

        let errors: Vec<String> = self
            .failed_topics
            .iter()
            .map(|e| format!("- {}: {}", e.topic_name, e.error))
            .collect();

        Some(format!(
            "Failed to sync {} topics:\n{}",
            self.failed_topics.len(),
            errors.join("\n")
        ))
    }
}

/// Service for synchronizing Omni semantic layer metadata
///
/// The OmniSyncService handles fetching metadata from the Omni API and storing it locally
/// in the project's metadata directories. It supports both individual topic synchronization
/// and full model synchronization workflows.
#[derive(Debug, Clone)]
pub struct OmniSyncService {
    api_client: OmniApiClient,
    storage: MetadataStorage,
    project_path: PathBuf,
    integration_name: String,
}

impl OmniSyncService {
    /// Create a new OmniSyncService instance
    ///
    /// # Arguments
    /// * `api_client` - Configured Omni API client for making requests
    /// * `project_path` - Path to the project root for metadata storage
    /// * `integration_name` - Name of the integration for metadata storage organization
    pub fn new<P: Into<PathBuf>>(
        api_client: OmniApiClient,
        project_path: P,
        integration_name: String,
    ) -> Self {
        let project_path = project_path.into();
        let storage = MetadataStorage::new(&project_path, integration_name.clone());

        Self {
            api_client,
            storage,
            project_path,
            integration_name,
        }
    }

    /// Synchronize metadata for a specific topic
    ///
    /// This method fetches topic metadata from the Omni API and saves it to the
    /// local .omni directory. It ensures the directory structure exists before saving.
    ///
    /// # Arguments
    /// * `model_id` - The Omni model ID to sync from
    /// * `topic_name` - The specific topic to synchronize
    ///
    /// # Returns
    /// * `Ok(())` if the topic was successfully synchronized
    /// * `Err(OmniError)` if the sync failed for any reason
    pub async fn sync_topic(&self, model_id: &str, topic_name: &str) -> Result<(), OmniError> {
        // Validate inputs
        if model_id.trim().is_empty() {
            return Err(OmniError::config_invalid(
                "model_id",
                "Model ID cannot be empty",
            ));
        }

        if topic_name.trim().is_empty() {
            return Err(OmniError::config_invalid(
                "topic_name",
                "Topic name cannot be empty",
            ));
        }

        // Ensure directory structure exists
        self.storage.ensure_directory_structure(model_id)?;

        // Fetch topic metadata from Omni API
        let topic_response = self
            .api_client
            .get_topic(model_id, topic_name)
            .await
            .map_err(|e| OmniError::sync_failed(topic_name, "fetch", &e.to_string()))?;

        // Validate API response
        if !topic_response.success {
            return Err(OmniError::sync_failed(
                topic_name,
                "validate response",
                "The topic may not exist or you may not have permission to access it",
            ));
        }

        // Convert API data to metadata structure
        let topic_metadata: TopicMetadata = topic_response.topic.into();

        // Validate the converted metadata
        if let Err(validation_error) = topic_metadata.validate_metadata() {
            return Err(OmniError::validation_failed(
                &format!("topic '{}'", topic_name),
                &validation_error.to_string(),
            ));
        }

        // Save the metadata to storage
        self.save_topic_metadata(model_id, &topic_metadata)?;

        Ok(())
    }

    /// Save topic metadata to the base metadata storage
    ///
    /// This method handles saving the topic metadata to the .omni directory
    /// and provides detailed error context if the save operation fails.
    ///
    /// # Arguments
    /// * `model_id` - The model ID for organizing the metadata
    /// * `topic_metadata` - The topic metadata to save
    ///
    /// # Returns
    /// * `Ok(())` if the metadata was successfully saved
    /// * `Err(OmniError)` if the save operation failed
    pub fn save_topic_metadata(
        &self,
        model_id: &str,
        topic_metadata: &TopicMetadata,
    ) -> Result<(), OmniError> {
        self.storage
            .save_base_metadata(model_id, topic_metadata)
            .map_err(|_e| {
                OmniError::StorageError(format!(
                    "Failed to save metadata for topic '{}' to .omni/{}/{}.yaml",
                    topic_metadata.name, model_id, topic_metadata.name
                ))
            })
    }

    /// Get a reference to the underlying API client
    ///
    /// This allows for advanced usage scenarios where direct API access is needed
    pub fn api_client(&self) -> &OmniApiClient {
        &self.api_client
    }

    /// Get a reference to the metadata storage
    ///
    /// This allows for direct access to storage operations if needed
    pub fn storage(&self) -> &MetadataStorage {
        &self.storage
    }

    /// Get the project path
    pub fn project_path(&self) -> &PathBuf {
        &self.project_path
    }

    /// Check if a topic has been synchronized (exists in base metadata)
    ///
    /// # Arguments
    /// * `model_id` - The model ID to check
    /// * `topic_name` - The topic name to check
    ///
    /// # Returns
    /// * `true` if the topic has been synchronized and exists in base metadata
    /// * `false` if the topic has not been synchronized
    pub fn is_topic_synchronized(&self, model_id: &str, topic_name: &str) -> bool {
        self.storage.base_metadata_exists(model_id, topic_name)
    }

    /// List all synchronized topics for a model
    ///
    /// # Arguments
    /// * `model_id` - The model ID to list topics for
    ///
    /// # Returns
    /// * `Ok(Vec<String>)` containing the list of synchronized topic names
    /// * `Err(OmniError)` if the listing operation failed
    pub fn list_synchronized_topics(&self, model_id: &str) -> Result<Vec<String>, OmniError> {
        self.storage.list_base_topics(model_id).map_err(|_e| {
            OmniError::StorageError(format!(
                "Failed to list synchronized topics for model '{}' in .omni/{}",
                model_id, model_id
            ))
        })
    }

    /// Create a metadata merger for this sync service
    ///
    /// This provides access to merged metadata functionality using the same
    /// project path and integration name as the sync service.
    pub fn create_metadata_merger(&self) -> MetadataMerger {
        MetadataMerger::new(&self.project_path, self.integration_name.clone())
    }

    /// Synchronize metadata for a specific topic
    ///
    /// This method synchronizes a single topic for a model from the Omni API.
    /// It provides progress tracking and error reporting.
    ///
    /// # Arguments
    /// * `model_id` - The Omni model ID to sync from
    /// * `topic_name` - The specific topic name to sync
    ///
    /// # Returns
    /// * `Ok(SyncResult)` containing sync statistics and any errors encountered
    /// * `Err(OmniError)` if the sync operation failed completely
    pub async fn sync_metadata(
        &self,
        model_id: &str,
        topic_name: &str,
    ) -> Result<SyncResult, OmniError> {
        // Validate inputs
        if model_id.trim().is_empty() {
            return Err(OmniError::ConfigError(
                "Model ID cannot be empty".to_string(),
            ));
        }

        if topic_name.trim().is_empty() {
            return Err(OmniError::ConfigError(
                "Topic name cannot be empty".to_string(),
            ));
        }

        // Ensure directory structure exists
        self.storage.ensure_directory_structure(model_id)?;

        // Initialize sync result for single topic
        let mut sync_result = SyncResult {
            model_id: model_id.to_string(),
            total_topics: 1,
            successful_topics: Vec::new(),
            failed_topics: Vec::new(),
            skipped_topics: Vec::new(),
        };

        // Sync the specific topic
        match self.sync_topic(model_id, topic_name).await {
            Ok(()) => {
                sync_result.successful_topics.push(topic_name.to_string());
            }
            Err(error) => {
                let topic_error = TopicSyncError {
                    topic_name: topic_name.to_string(),
                    error: error.to_string(),
                };
                sync_result.failed_topics.push(topic_error);
            }
        }

        Ok(sync_result)
    }
}
